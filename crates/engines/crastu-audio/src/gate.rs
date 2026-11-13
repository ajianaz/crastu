use crate::hysteresis::Hysteresis;
use crate::level::level_of;

/// What the gate says about one frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GateEvent {
    Silence,
    /// The first frame of speech.
    SpeechStart,
    Speech,
    /// The first frame after speech ended (the hangover ran out).
    SpeechEnd,
}

impl GateEvent {
    /// Whether the frame belongs to speech (a start counts, an end does not).
    pub fn is_speech(self) -> bool {
        matches!(self, GateEvent::SpeechStart | GateEvent::Speech)
    }
}

/// Decides, frame by frame, whether there is speech. The energy gate below is the first
/// implementation; `SileroGate` (a model instead of an energy) is the other, behind the same trait.
pub trait VoiceGate: Send {
    fn process(&mut self, frame: &[f32]) -> GateEvent;
    fn reset(&mut self);
}

/// Thresholds of the [`EnergyGate`]. `open_db` is above `close_db` (hysteresis): a frame must be
/// loud to start speech, but only has to stay above the lower bound to continue it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GateConfig {
    /// Speech starts when the frame's RMS reaches this level.
    pub open_db: f32,
    /// Speech continues while the RMS stays at or above this level.
    pub close_db: f32,
    /// Frames below `close_db` that still count as speech, so a short pause does not cut it.
    pub hangover_frames: u32,
}

impl Default for GateConfig {
    fn default() -> Self {
        GateConfig {
            open_db: -40.0,
            close_db: -48.0,
            hangover_frames: 15, // 300 ms at 20 ms frames
        }
    }
}

/// A gate on frame RMS with hysteresis and a hangover.
#[derive(Clone, Debug)]
pub struct EnergyGate {
    cfg: GateConfig,
    state: Hysteresis,
}

impl EnergyGate {
    pub fn new(cfg: GateConfig) -> Self {
        EnergyGate {
            cfg,
            state: Hysteresis::new(cfg.hangover_frames),
        }
    }
}

impl Default for EnergyGate {
    fn default() -> Self {
        EnergyGate::new(GateConfig::default())
    }
}

impl VoiceGate for EnergyGate {
    fn process(&mut self, frame: &[f32]) -> GateEvent {
        let rms = level_of(frame).rms_db;
        self.state
            .step(rms >= self.cfg.open_db, rms >= self.cfg.close_db)
    }

    fn reset(&mut self) {
        self.state.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Constant frames: rms in dB equals the amplitude's dB.
    fn frame(db: f32) -> Vec<f32> {
        vec![10f32.powf(db / 20.0); 320]
    }

    fn gate(hang: u32) -> EnergyGate {
        EnergyGate::new(GateConfig {
            open_db: -40.0,
            close_db: -48.0,
            hangover_frames: hang,
        })
    }

    #[test]
    fn silence_stays_silent_and_loud_starts_speech() {
        let mut g = gate(2);
        assert_eq!(g.process(&frame(-70.0)), GateEvent::Silence);
        assert_eq!(g.process(&frame(-30.0)), GateEvent::SpeechStart);
        assert_eq!(g.process(&frame(-30.0)), GateEvent::Speech);
    }

    #[test]
    fn hysteresis_between_the_thresholds() {
        let mut g = gate(0);
        // -44 dB is below open, so it does not start speech...
        assert_eq!(g.process(&frame(-44.0)), GateEvent::Silence);
        assert_eq!(g.process(&frame(-30.0)), GateEvent::SpeechStart);
        // ...but it continues it.
        assert_eq!(g.process(&frame(-44.0)), GateEvent::Speech);
        assert_eq!(g.process(&frame(-44.0)), GateEvent::Speech);
    }

    #[test]
    fn hangover_bridges_a_short_pause_and_then_ends() {
        let mut g = gate(2);
        g.process(&frame(-30.0));
        assert_eq!(g.process(&frame(-70.0)), GateEvent::Speech);
        assert_eq!(g.process(&frame(-70.0)), GateEvent::Speech);
        assert_eq!(g.process(&frame(-70.0)), GateEvent::SpeechEnd);
        assert_eq!(g.process(&frame(-70.0)), GateEvent::Silence);
    }

    #[test]
    fn speech_resuming_resets_the_hangover() {
        let mut g = gate(2);
        g.process(&frame(-30.0));
        g.process(&frame(-70.0));
        g.process(&frame(-70.0));
        assert_eq!(g.process(&frame(-30.0)), GateEvent::Speech);
        assert_eq!(g.process(&frame(-70.0)), GateEvent::Speech);
        assert_eq!(g.process(&frame(-70.0)), GateEvent::Speech);
        assert_eq!(g.process(&frame(-70.0)), GateEvent::SpeechEnd);
    }

    #[test]
    fn reset_closes_the_gate() {
        let mut g = gate(5);
        g.process(&frame(-30.0));
        g.reset();
        assert_eq!(g.process(&frame(-70.0)), GateEvent::Silence);
    }

    #[test]
    fn is_speech_marks_start_but_not_end() {
        assert!(GateEvent::SpeechStart.is_speech());
        assert!(GateEvent::Speech.is_speech());
        assert!(!GateEvent::SpeechEnd.is_speech());
        assert!(!GateEvent::Silence.is_speech());
    }
}
