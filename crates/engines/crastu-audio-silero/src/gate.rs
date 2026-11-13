//! A voice gate driven by a speech probability model (Silero VAD), behind a [`Predictor`] trait
//! so the logic is tested without a model or a runtime.
//!
//! The model takes windows of 512 samples at 16 kHz (32 ms; checked 2026-11-13 against the ONNX
//! file and `utils_vad.py` of the catalogued revision, see `silero_ort`), while this crate's
//! frames are 320 samples (20 ms). The gate keeps a carry buffer: it collects samples, runs the
//! predictor on every complete window, and answers each frame with the newest probability (so a
//! frame can be answered with the probability of a window that ended up to 32 ms before it; the
//! very first frame is answered before any window is complete, as silence). The start, continue
//! and hangover logic is the one of the energy gate (`crastu_audio::Hysteresis`).

use crastu_audio::{GateEvent, Hysteresis, VoiceGate};

/// Samples per model window at 16 kHz.
pub const WINDOW_SAMPLES: usize = 512;

/// Turns one window of exactly [`WINDOW_SAMPLES`] samples into a speech probability in 0..=1.
/// An implementation carries the model's own state between windows.
pub trait Predictor: Send {
    fn predict(&mut self, window: &[f32]) -> Result<f32, String>;
    /// Forgets the state carried between windows (a new stream starts).
    fn reset(&mut self);
}

/// Thresholds of the [`SileroGate`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SileroConfig {
    /// Speech starts when the probability reaches this value (0.01..=0.99; Silero's default 0.5).
    pub threshold: f32,
    /// Speech continues while the probability stays at or above `threshold - release_margin`
    /// (Silero's own default is 0.15, with a floor of 0.01).
    pub release_margin: f32,
    /// Frames below the release level that still count as speech.
    pub hangover_frames: u32,
}

impl Default for SileroConfig {
    fn default() -> Self {
        SileroConfig {
            threshold: 0.5,
            release_margin: 0.15,
            hangover_frames: 15, // 300 ms at 20 ms frames, as the energy gate
        }
    }
}

impl SileroConfig {
    pub fn with_threshold(mut self, threshold: f32) -> Self {
        self.threshold = threshold;
        self
    }

    /// The configuration with nonsense values (NaN, out of range) replaced by usable ones.
    fn sanitised(self) -> Self {
        let d = SileroConfig::default();
        let pick = |v: f32, lo: f32, hi: f32, fallback: f32| {
            if v.is_finite() {
                v.clamp(lo, hi)
            } else {
                fallback
            }
        };
        SileroConfig {
            threshold: pick(self.threshold, 0.01, 0.99, d.threshold),
            release_margin: pick(self.release_margin, 0.0, 0.98, d.release_margin),
            hangover_frames: self.hangover_frames,
        }
    }

    fn release_level(&self) -> f32 {
        (self.threshold - self.release_margin).max(0.01)
    }
}

/// A [`VoiceGate`] on a speech probability model.
#[derive(Debug)]
pub struct SileroGate<P: Predictor> {
    predictor: P,
    cfg: SileroConfig,
    carry: Vec<f32>,
    probability: f32,
    state: Hysteresis,
    errors: u64,
    last_error: Option<String>,
}

impl<P: Predictor> SileroGate<P> {
    pub fn new(predictor: P, cfg: SileroConfig) -> Self {
        let cfg = cfg.sanitised();
        SileroGate {
            predictor,
            cfg,
            carry: Vec::with_capacity(2 * WINDOW_SAMPLES),
            probability: 0.0,
            state: Hysteresis::new(cfg.hangover_frames),
            errors: 0,
            last_error: None,
        }
    }

    /// The probability of the newest complete window (0 before the first one).
    pub fn probability(&self) -> f32 {
        self.probability
    }

    /// How many windows the predictor failed on. A failed window counts as probability 0, so a
    /// broken model ends speech after the hangover instead of holding it open for ever.
    pub fn predict_errors(&self) -> u64 {
        self.errors
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    fn fail(&mut self, reason: String) {
        self.errors += 1;
        self.last_error = Some(reason);
        self.probability = 0.0;
    }
}

impl<P: Predictor> VoiceGate for SileroGate<P> {
    /// Adds the frame to the carry buffer and runs the predictor on every complete window. When
    /// one call completes several windows (a frame longer than a window), only the newest
    /// probability counts: the frames this crate's converter makes (320 samples) never do.
    fn process(&mut self, frame: &[f32]) -> GateEvent {
        self.carry.extend_from_slice(frame);
        let mut used = 0;
        while self.carry.len() - used >= WINDOW_SAMPLES {
            let window = &self.carry[used..used + WINDOW_SAMPLES];
            match self.predictor.predict(window) {
                // A probability outside 0..=1 is clamped; NaN is a failure of the model.
                Ok(p) if !p.is_nan() => self.probability = p.clamp(0.0, 1.0),
                Ok(_) => self.fail("the model returned NaN".to_string()),
                Err(e) => self.fail(e),
            }
            used += WINDOW_SAMPLES;
        }
        self.carry.drain(..used);
        self.state.step(
            self.probability >= self.cfg.threshold,
            self.probability >= self.cfg.release_level(),
        )
    }

    fn reset(&mut self) {
        self.carry.clear();
        self.probability = 0.0;
        self.state.reset();
        self.predictor.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct Log {
        windows: Vec<Vec<f32>>,
        resets: usize,
    }

    /// Answers from a script, one entry per window; `Err` entries fail.
    struct Fake {
        script: Vec<Result<f32, String>>,
        next: usize,
        log: Arc<Mutex<Log>>,
    }

    impl Fake {
        fn new(script: Vec<Result<f32, String>>) -> (Self, Arc<Mutex<Log>>) {
            let log = Arc::new(Mutex::new(Log::default()));
            let fake = Fake {
                script,
                next: 0,
                log: Arc::clone(&log),
            };
            (fake, log)
        }
    }

    impl Predictor for Fake {
        fn predict(&mut self, window: &[f32]) -> Result<f32, String> {
            self.log.lock().unwrap().windows.push(window.to_vec());
            let out = self.script[self.next.min(self.script.len() - 1)].clone();
            self.next += 1;
            out
        }

        fn reset(&mut self) {
            self.log.lock().unwrap().resets += 1;
        }
    }

    fn probs(p: &[f32]) -> Vec<Result<f32, String>> {
        p.iter().map(|v| Ok(*v)).collect()
    }

    /// `n` frames of 320 samples numbered from `start`, so a window's content shows its place.
    fn ramp(start: usize, n: usize) -> Vec<f32> {
        (start..start + n).map(|i| i as f32).collect()
    }

    #[test]
    fn frames_of_320_are_cut_into_contiguous_windows_of_512_with_a_carry() {
        let (fake, log) = Fake::new(probs(&[0.0]));
        let mut gate = SileroGate::new(fake, SileroConfig::default());
        gate.process(&ramp(0, 320));
        assert!(
            log.lock().unwrap().windows.is_empty(),
            "320 < 512: no window yet"
        );
        gate.process(&ramp(320, 320)); // 640: one window, 128 carried
        gate.process(&ramp(640, 320)); // 448: none
        gate.process(&ramp(960, 320)); // 768: one window, 256 carried
        let log = log.lock().unwrap();
        assert_eq!(log.windows.len(), 2);
        assert_eq!(log.windows[0], ramp(0, 512));
        assert_eq!(log.windows[1], ramp(512, 512));
    }

    #[test]
    fn a_frame_longer_than_a_window_runs_several_windows() {
        let (fake, log) = Fake::new(probs(&[0.0]));
        let mut gate = SileroGate::new(fake, SileroConfig::default());
        gate.process(&ramp(0, 1100));
        assert_eq!(log.lock().unwrap().windows.len(), 2);
    }

    #[test]
    fn threshold_starts_speech_and_the_lower_release_level_keeps_it() {
        // One window per frame here: feed 512 samples at a time.
        let (fake, _) = Fake::new(probs(&[0.2, 0.6, 0.4, 0.4, 0.3, 0.3]));
        let mut gate = SileroGate::new(
            fake,
            SileroConfig {
                hangover_frames: 0,
                ..SileroConfig::default()
            },
        );
        let f = ramp(0, 512);
        assert_eq!(gate.process(&f), GateEvent::Silence); // 0.2 < 0.5
        assert_eq!(gate.process(&f), GateEvent::SpeechStart); // 0.6
        assert_eq!(gate.process(&f), GateEvent::Speech); // 0.4 >= 0.35
        assert_eq!(gate.process(&f), GateEvent::Speech);
        assert_eq!(gate.process(&f), GateEvent::SpeechEnd); // 0.3 < 0.35, no hangover
        assert_eq!(gate.process(&f), GateEvent::Silence);
    }

    #[test]
    fn a_probability_between_the_levels_does_not_start_speech() {
        let (fake, _) = Fake::new(probs(&[0.4]));
        let mut gate = SileroGate::new(fake, SileroConfig::default());
        assert_eq!(gate.process(&ramp(0, 512)), GateEvent::Silence);
    }

    #[test]
    fn hangover_bridges_a_pause_like_the_energy_gate() {
        let (fake, _) = Fake::new(probs(&[0.9, 0.0, 0.0, 0.0, 0.0]));
        let mut gate = SileroGate::new(
            fake,
            SileroConfig {
                hangover_frames: 2,
                ..SileroConfig::default()
            },
        );
        let f = ramp(0, 512);
        assert_eq!(gate.process(&f), GateEvent::SpeechStart);
        assert_eq!(gate.process(&f), GateEvent::Speech);
        assert_eq!(gate.process(&f), GateEvent::Speech);
        assert_eq!(gate.process(&f), GateEvent::SpeechEnd);
        assert_eq!(gate.process(&f), GateEvent::Silence);
    }

    #[test]
    fn a_frame_without_a_new_window_repeats_the_newest_probability() {
        let (fake, _) = Fake::new(probs(&[0.9]));
        let mut gate = SileroGate::new(fake, SileroConfig::default());
        assert_eq!(gate.process(&ramp(0, 320)), GateEvent::Silence); // no window yet
        assert_eq!(gate.process(&ramp(320, 320)), GateEvent::SpeechStart);
        assert_eq!(gate.process(&ramp(640, 320)), GateEvent::Speech); // no new window: 0.9 held
        assert!((gate.probability() - 0.9).abs() < 1e-6);
    }

    #[test]
    fn reset_clears_the_carry_the_gate_and_the_model_state() {
        let (fake, log) = Fake::new(probs(&[0.9]));
        let mut gate = SileroGate::new(fake, SileroConfig::default());
        gate.process(&ramp(0, 700)); // one window, 188 carried, speech open
        gate.reset();
        assert_eq!(log.lock().unwrap().resets, 1);
        assert_eq!(gate.probability(), 0.0);
        // The carried 188 samples are gone: 400 more make no window.
        let before = log.lock().unwrap().windows.len();
        assert_eq!(gate.process(&ramp(0, 400)), GateEvent::Silence);
        assert_eq!(log.lock().unwrap().windows.len(), before);
    }

    #[test]
    fn a_failing_predictor_counts_errors_and_ends_speech_after_the_hangover() {
        let script = vec![Ok(0.9), Err("boom".to_string())];
        let (fake, _) = Fake::new(script);
        let mut gate = SileroGate::new(
            fake,
            SileroConfig {
                hangover_frames: 1,
                ..SileroConfig::default()
            },
        );
        let f = ramp(0, 512);
        assert_eq!(gate.process(&f), GateEvent::SpeechStart);
        assert_eq!(gate.process(&f), GateEvent::Speech); // hangover
        assert_eq!(gate.process(&f), GateEvent::SpeechEnd);
        assert_eq!(gate.predict_errors(), 2);
        assert_eq!(gate.last_error(), Some("boom"));
    }

    #[test]
    fn nonsense_configuration_is_replaced() {
        let c = SileroConfig {
            threshold: f32::NAN,
            release_margin: 5.0,
            hangover_frames: 3,
        }
        .sanitised();
        assert_eq!(c.threshold, 0.5);
        assert!((c.release_margin - 0.98).abs() < 1e-6);
        assert!(c.release_level() >= 0.01);
        assert_eq!(
            SileroConfig::default()
                .with_threshold(2.0)
                .sanitised()
                .threshold,
            0.99
        );
    }

    #[test]
    fn nan_and_out_of_range_probabilities_are_handled() {
        let (fake, _) = Fake::new(probs(&[f32::NAN, 7.0, -3.0]));
        let mut gate = SileroGate::new(
            fake,
            SileroConfig {
                hangover_frames: 0,
                ..SileroConfig::default()
            },
        );
        let f = ramp(0, 512);
        assert_eq!(gate.process(&f), GateEvent::Silence);
        assert_eq!(gate.predict_errors(), 1);
        assert_eq!(gate.last_error(), Some("the model returned NaN"));
        assert_eq!(gate.process(&f), GateEvent::SpeechStart);
        assert_eq!(gate.probability(), 1.0);
        assert_eq!(gate.process(&f), GateEvent::SpeechEnd);
        assert_eq!(gate.probability(), 0.0);
    }

    #[test]
    fn an_empty_frame_changes_nothing() {
        let (fake, log) = Fake::new(probs(&[0.9]));
        let mut gate = SileroGate::new(fake, SileroConfig::default());
        assert_eq!(gate.process(&[]), GateEvent::Silence);
        assert!(log.lock().unwrap().windows.is_empty());
    }

    #[test]
    fn a_higher_threshold_needs_a_stronger_probability() {
        let (fake, _) = Fake::new(probs(&[0.6]));
        let cfg = SileroConfig::default().with_threshold(0.8);
        let mut gate = SileroGate::new(fake, cfg);
        assert_eq!(gate.process(&ramp(0, 512)), GateEvent::Silence);
    }
}
