#![allow(dead_code)]

use std::f64::consts::TAU;
use std::sync::Arc;

use crastu_mix::{Automation, Clip, MemorySource, MixConfig, Mixer, PcmSource, Timeline, Track};

pub const RATE: u32 = 48_000;

/// Mono sine, `amp` peak.
pub fn sine_mono(freq: f64, amp: f32, frames: usize, rate: u32) -> Vec<f32> {
    (0..frames)
        .map(|i| amp * (TAU * freq * i as f64 / f64::from(rate)).sin() as f32)
        .collect()
}

/// Stereo sine, the right side a little lower and at another frequency.
pub fn sine_stereo(freq: f64, amp: f32, frames: usize, rate: u32) -> Vec<f32> {
    let l = sine_mono(freq, amp, frames, rate);
    let r = sine_mono(freq * 1.5, amp * 0.8, frames, rate);
    l.iter().zip(&r).flat_map(|(a, b)| [*a, *b]).collect()
}

/// A voice-like mono signal: a 180 Hz tone with harmonics under a slow syllable envelope.
pub fn voice(frames: usize, rate: u32, amp: f32) -> Vec<f32> {
    (0..frames)
        .map(|i| {
            let t = i as f64 / f64::from(rate);
            let env = 0.5 + 0.5 * (TAU * 3.0 * t).sin();
            let s = (TAU * 180.0 * t).sin()
                + 0.5 * (TAU * 360.0 * t).sin()
                + 0.25 * (TAU * 900.0 * t).sin();
            amp * (env * s / 1.75) as f32
        })
        .collect()
}

/// Deterministic white noise in -1..1.
pub struct Lcg(pub u64);

impl Lcg {
    pub fn next_f32(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 40) as f32 / (1u64 << 23) as f32) - 1.0
    }
}

pub fn mem(channels: usize, data: Vec<f32>) -> Box<dyn PcmSource> {
    Box::new(MemorySource::new(channels, Arc::from(data)))
}

/// Three seconds: two voice clips with fades, music with a fade-in, a cut and a second music
/// clip, and a music gain ramp.
pub fn scene(master_gain: f32) -> (Timeline, Vec<Box<dyn PcmSource>>, MixConfig) {
    let n = 3 * RATE as usize;
    let sources = vec![
        mem(1, voice(n, RATE, 0.7)),
        mem(2, sine_stereo(110.0, 0.6, n, RATE)),
        mem(2, sine_stereo(330.0, 0.5, n, RATE)),
    ];
    let r = u64::from(RATE);
    let mut v1 = Clip::new(Track::Voice, 0, r * 4 / 10, r * 8 / 10);
    v1.fade_in = 480;
    v1.fade_out = 960;
    let mut v2 = Clip::new(Track::Voice, 0, r * 15 / 10, r * 8 / 10);
    v2.source_offset = 5000;
    v2.gain = 0.8;
    let mut m1 = Clip::new(Track::Music, 1, 0, r * 2);
    m1.fade_in = 4800;
    m1.fade_out = 2400;
    let mut m2 = Clip::new(Track::Music, 2, r * 2 - 1000, r + 1000);
    m2.fade_in = 3000;
    m2.fade_out = 6000;
    let timeline = Timeline {
        clips: vec![v1, v2, m1, m2],
        music_gain: Automation::new(vec![(0, 0.5), (r, 0.5), (r * 2, 1.0)]),
    };
    (
        timeline,
        sources,
        MixConfig {
            master_gain,
            ..MixConfig::default()
        },
    )
}

pub fn scene_mixer(master_gain: f32) -> Mixer {
    let (t, s, c) = scene(master_gain);
    Mixer::new(t, s, &c).unwrap()
}

pub fn scene_len() -> u64 {
    3 * u64::from(RATE)
}

pub fn bits(v: &[f32]) -> Vec<u32> {
    v.iter().map(|x| x.to_bits()).collect()
}
