//! The graph: sources -> clip gain -> voice and music buses -> ducking -> sum -> master gain ->
//! true-peak limiter. One [`Mixer`] serves the output device callback ([`Mixer::process`]) and
//! offline export ([`Mixer::bounce`]); `bounce` is `seek` plus `process`.

use std::ops::Range;

use crate::MixError;
use crate::curve::db_to_gain;
use crate::duck::Ducker;
use crate::limiter::Limiter;
use crate::source::PcmSource;
use crate::timeline::{Clip, Event, EventKind, Timeline, Track};

/// Frames rendered per internal step; `process` accepts any block size and splits it.
const CHUNK: usize = 512;

/// Mixer settings. Times are in milliseconds and become whole samples at `sample_rate`.
#[derive(Clone, Debug, PartialEq)]
pub struct MixConfig {
    /// Sample rate of the sources and of the output, in Hz.
    pub sample_rate: u32,
    /// Linear gain before the limiter (see `gain_to_target`).
    pub master_gain: f32,
    /// Limiter ceiling in dBTP.
    pub ceiling_db: f64,
    /// Limiter look-ahead in milliseconds.
    pub lookahead_ms: f64,
    /// Music level under the voice in dB (negative).
    pub duck_db: f64,
    /// How long before the voice the music starts to fall, in milliseconds.
    pub duck_attack_ms: f64,
    /// How long the music takes to come back after the voice, in milliseconds.
    pub duck_release_ms: f64,
}

impl Default for MixConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48_000,
            master_gain: 1.0,
            ceiling_db: -1.0,
            lookahead_ms: 1.0,
            duck_db: -12.0,
            duck_attack_ms: 30.0,
            duck_release_ms: 400.0,
        }
    }
}

impl MixConfig {
    fn samples(&self, ms: f64) -> u64 {
        (ms.max(0.0) * f64::from(self.sample_rate) / 1000.0).round() as u64
    }
}

/// Timeline and sources to interleaved stereo, before the limiter. No allocation after `new`.
struct Renderer {
    clips: Vec<Clip>,
    events: Vec<Event>,
    sources: Vec<Box<dyn PcmSource>>,
    ducker: Ducker,
    timeline: Timeline,
    cursor: usize,
    active: Vec<bool>,
    src: Vec<f32>,
    voice: Vec<f32>,
    music: Vec<f32>,
    master: f32,
}

impl Renderer {
    fn reset(&mut self) {
        self.cursor = 0;
        self.active.fill(false);
    }

    /// Renders `out.len() / 2` frames starting at timeline sample `pos`. Calls must follow each
    /// other in position order after a `reset`.
    fn render(&mut self, pos: u64, out: &mut [f32]) {
        let frames = out.len() / 2;
        let mut done = 0;
        while done < frames {
            let p = pos.saturating_add(done as u64);
            while let Some(e) = self.events.get(self.cursor).filter(|e| e.pos <= p) {
                self.active[e.clip] = e.kind == EventKind::Start;
                self.cursor += 1;
            }
            let until_event = self.events.get(self.cursor).map_or(u64::MAX, |e| e.pos - p);
            let n = (frames - done).min(usize::try_from(until_event).unwrap_or(usize::MAX));
            self.segment(p, &mut out[done * 2..(done + n) * 2]);
            done += n;
        }
    }

    /// One stretch in which the set of playing clips does not change.
    fn segment(&mut self, p: u64, out: &mut [f32]) {
        let n = out.len() / 2;
        self.voice[..n * 2].fill(0.0);
        self.music[..n * 2].fill(0.0);
        for (i, clip) in self.clips.iter().enumerate() {
            if !self.active[i] {
                continue;
            }
            let source = &mut self.sources[clip.source];
            let channels = source.channels();
            let first = clip.source_offset.saturating_add(p - clip.start);
            source.read(first, &mut self.src[..n * channels]);
            let bus = if clip.track == Track::Voice {
                &mut self.voice
            } else {
                &mut self.music
            };
            for k in 0..n {
                let g = clip.gain_at(p.saturating_add(k as u64));
                let (l, r) = if channels == 1 {
                    (self.src[k], self.src[k])
                } else {
                    (self.src[2 * k], self.src[2 * k + 1])
                };
                bus[2 * k] += l * g;
                bus[2 * k + 1] += r * g;
            }
        }
        for k in 0..n {
            let at = p.saturating_add(k as u64);
            let music = self.timeline.music_gain.value_at(at) * self.ducker.gain_at(at);
            for c in 0..2 {
                // A source that hands out NaN or infinity must not reach the limiter or the meter.
                let v = (self.voice[2 * k + c] + self.music[2 * k + c] * music) * self.master;
                out[2 * k + c] = if v.is_finite() { v } else { 0.0 };
            }
        }
    }
}

/// The mixer. Create it off the audio thread; `process` and `seek` are safe to call from it.
pub struct Mixer {
    renderer: Renderer,
    limiter: Limiter,
    tmp: Vec<f32>,
    pos: u64,
    feed: u64,
    primed: bool,
}

impl std::fmt::Debug for Mixer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mixer")
            .field("pos", &self.pos)
            .field("clips", &self.renderer.clips.len())
            .finish()
    }
}

impl Mixer {
    /// Checks the timeline against the sources and builds the graph.
    ///
    /// # Errors
    /// [`MixError`] for a bad sample rate, a clip naming a missing source, a source that is not
    /// mono or stereo, or a gain that is not finite.
    pub fn new(
        timeline: Timeline,
        sources: Vec<Box<dyn PcmSource>>,
        config: &MixConfig,
    ) -> Result<Self, MixError> {
        if config.sample_rate == 0 {
            return Err(MixError::SampleRate);
        }
        if sources.iter().any(|s| !(1..=2).contains(&s.channels())) {
            return Err(MixError::Channels);
        }
        for (i, c) in timeline.clips.iter().enumerate() {
            if c.source >= sources.len() {
                return Err(MixError::UnknownSource {
                    clip: i,
                    source: c.source,
                });
            }
            if !c.gain.is_finite() {
                return Err(MixError::Gain);
            }
        }
        if !timeline.music_gain.is_finite() || !config.master_gain.is_finite() {
            return Err(MixError::Gain);
        }
        let events = timeline.events();
        let ducker = Ducker::new(
            timeline.voice_spans(),
            db_to_gain(config.duck_db.min(0.0)),
            config.samples(config.duck_attack_ms),
            config.samples(config.duck_release_ms),
        );
        let limiter = Limiter::new(
            db_to_gain(config.ceiling_db),
            config.samples(config.lookahead_ms) as usize,
        );
        let renderer = Renderer {
            clips: timeline.clips.clone(),
            events,
            sources,
            ducker,
            active: vec![false; timeline.clips.len()],
            timeline,
            cursor: 0,
            src: vec![0.0; CHUNK * 2],
            voice: vec![0.0; CHUNK * 2],
            music: vec![0.0; CHUNK * 2],
            master: config.master_gain,
        };
        Ok(Self {
            renderer,
            limiter,
            tmp: vec![0.0; CHUNK * 2],
            pos: 0,
            feed: 0,
            primed: false,
        })
    }

    /// Frames between the mixed signal and the output caused by the limiter's look-ahead; the
    /// mixer hides it by rendering that far ahead, so output frame `p` is timeline sample `p`.
    #[must_use]
    pub fn latency(&self) -> usize {
        self.limiter.latency()
    }

    /// The timeline sample the next `process` call starts at.
    #[must_use]
    pub fn position(&self) -> u64 {
        self.pos
    }

    /// Moves to timeline sample `pos`. The limiter starts fresh there, so the first few
    /// milliseconds can differ from continuous playback if the limiter was working, and a seek
    /// into the middle of a clip starts with that clip's signal at once (no fade), as a cut would.
    pub fn seek(&mut self, pos: u64) {
        self.pos = pos;
        self.primed = false;
    }

    /// Fills `out` (interleaved stereo; an odd last sample is zeroed) with the next frames and
    /// advances. Any length is fine and gives the same samples as any other split. Does not
    /// allocate or lock.
    pub fn process(&mut self, out: &mut [f32]) {
        if !self.primed {
            self.prime();
        }
        let frames = out.len() / 2;
        let mut done = 0;
        while done < frames {
            let n = (frames - done).min(CHUNK);
            self.renderer.render(self.feed, &mut self.tmp[..n * 2]);
            for k in 0..n {
                let y = self.limiter.push([self.tmp[2 * k], self.tmp[2 * k + 1]]);
                out[(done + k) * 2] = y[0];
                out[(done + k) * 2 + 1] = y[1];
            }
            self.feed = self.feed.saturating_add(n as u64);
            done += n;
        }
        if out.len() % 2 == 1 {
            out[frames * 2] = 0.0;
        }
        self.pos = self.pos.saturating_add(frames as u64);
    }

    /// Renders the frames of `range` offline, as interleaved stereo. The same code as live
    /// playback started at `range.start`, so the samples are the ones the device would play from
    /// there (the limiter starts fresh at `range.start`; see `seek`). Spans above 2^29 samples
    /// (about 90 minutes of stereo at 48 kHz) return an empty vector: render those in pieces
    /// with `process`.
    pub fn bounce(&mut self, range: Range<u64>) -> Vec<f32> {
        let Some(samples) = usize::try_from(range.end.saturating_sub(range.start))
            .ok()
            .and_then(|f| f.checked_mul(2))
            .filter(|s| *s <= 1 << 29)
        else {
            return Vec::new(); // a span that cannot be held in memory
        };
        let mut out = vec![0.0; samples];
        self.seek(range.start);
        self.process(&mut out);
        out
    }

    /// Fills the limiter's look-ahead from the current position.
    fn prime(&mut self) {
        self.renderer.reset();
        self.limiter.reset();
        let mut at = self.pos;
        let mut left = self.limiter.latency();
        while left > 0 {
            let n = left.min(CHUNK);
            self.renderer.render(at, &mut self.tmp[..n * 2]);
            for k in 0..n {
                self.limiter.push([self.tmp[2 * k], self.tmp[2 * k + 1]]);
            }
            at = at.saturating_add(n as u64);
            left -= n;
        }
        self.feed = at;
        self.primed = true;
    }
}
