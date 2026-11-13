//! Loudness measurement and targets, split from `crastu-mix` to keep both crates under the size limit.
//! Safe code, std only; compiles for iOS and Android.
//!
//! Integrated loudness after ITU-R BS.1770-4: K-weighting (a high-shelf and a high-pass, both
//! biquads), mean square over 400 ms blocks at a 100 ms hop, an absolute gate at -70 LUFS and a
//! relative gate 10 LU under the gated mean. Channels are the front pair, weight 1 each.

use std::f64::consts::PI;

/// Loudness target of the social-media presets (an example constant: platforms normalise to about
/// this value).
pub const SOCIAL_TARGET_LUFS: f64 = -14.0;
/// EBU R 128 broadcast target.
pub const BROADCAST_TARGET_LUFS: f64 = -23.0;

const ABSOLUTE_GATE: f64 = -70.0;
const RELATIVE_GATE: f64 = -10.0;
const OFFSET: f64 = -0.691;

#[derive(Clone, Copy, Debug)]
struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
}

/// The two K-weighting stages for `rate`, designed with the bilinear transform from the analogue
/// parameters of BS.1770 (they reproduce the standard's 48 kHz coefficients).
fn k_weighting(rate: f64) -> [Biquad; 2] {
    let (f0, gain_db, q) = (
        1_681.974_450_955_533,
        3.999_843_853_973_347,
        0.707_175_236_955_419_6,
    );
    let k = (PI * f0 / rate).tan();
    let vh = 10f64.powf(gain_db / 20.0);
    let vb = vh.powf(0.499_666_774_154_541_6);
    let a0 = 1.0 + k / q + k * k;
    let shelf = Biquad {
        b: [
            (vh + vb * k / q + k * k) / a0,
            2.0 * (k * k - vh) / a0,
            (vh - vb * k / q + k * k) / a0,
        ],
        a: [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
    };
    let (f0, q) = (38.135_470_876_024_44, 0.500_327_037_323_877_3);
    let k = (PI * f0 / rate).tan();
    let a0 = 1.0 + k / q + k * k;
    let high_pass = Biquad {
        b: [1.0, -2.0, 1.0],
        a: [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
    };
    [shelf, high_pass]
}

#[derive(Clone, Copy, Debug, Default)]
struct State {
    z: [[f64; 2]; 2],
}

impl State {
    /// Transposed direct form II through both stages.
    fn run(&mut self, stages: &[Biquad; 2], x: f64) -> f64 {
        let mut v = x;
        for (st, z) in stages.iter().zip(self.z.iter_mut()) {
            let y = st.b[0] * v + z[0];
            z[0] = st.b[1] * v - st.a[0] * y + z[1];
            z[1] = st.b[2] * v - st.a[1] * y;
            v = y;
        }
        v
    }
}

/// Streaming integrated-loudness meter for mono or stereo.
#[derive(Clone, Debug)]
pub struct LoudnessMeter {
    channels: usize,
    stages: [Biquad; 2],
    state: [State; 2],
    hop: usize,
    in_hop: usize,
    acc: f64,
    /// Sum over channels of squared filtered samples, per completed 100 ms hop.
    hops: Vec<f64>,
}

impl LoudnessMeter {
    /// `channels` is 1 or 2 (anything else is treated as 2); `rate` in Hz.
    #[must_use]
    pub fn new(rate: u32, channels: usize) -> Self {
        let rate = rate.max(1);
        Self {
            channels: if channels == 1 { 1 } else { 2 },
            stages: k_weighting(f64::from(rate)),
            state: [State::default(); 2],
            hop: ((f64::from(rate) * 0.1).round() as usize).max(1),
            in_hop: 0,
            acc: 0.0,
            hops: Vec::new(),
        }
    }

    /// Feeds interleaved samples; a trailing partial frame is ignored.
    pub fn push(&mut self, interleaved: &[f32]) {
        for frame in interleaved.chunks_exact(self.channels) {
            for (c, s) in frame.iter().enumerate() {
                // NaN or infinity would poison the filter state for the rest of the stream.
                let x = if s.is_finite() { f64::from(*s) } else { 0.0 };
                let y = self.state[c].run(&self.stages, x);
                self.acc += y * y;
            }
            self.in_hop += 1;
            if self.in_hop == self.hop {
                self.hops.push(self.acc);
                self.acc = 0.0;
                self.in_hop = 0;
            }
        }
    }

    /// Gated integrated loudness in LUFS of everything pushed, or `None` when there is not one
    /// full 400 ms block above the -70 LUFS gate.
    #[must_use]
    pub fn integrated_lufs(&self) -> Option<f64> {
        let block_len = (self.hop * 4) as f64;
        let blocks: Vec<f64> = self
            .hops
            .windows(4)
            .map(|w| w.iter().sum::<f64>() / block_len)
            .collect();
        let lufs = |e: f64| OFFSET + 10.0 * e.log10();
        let mean_above = |threshold: f64| -> Option<f64> {
            let kept: Vec<f64> = blocks
                .iter()
                .copied()
                .filter(|e| *e > 0.0 && lufs(*e) > threshold)
                .collect();
            (!kept.is_empty()).then(|| kept.iter().sum::<f64>() / kept.len() as f64)
        };
        let first = mean_above(ABSOLUTE_GATE)?;
        let second = mean_above(ABSOLUTE_GATE.max(lufs(first) + RELATIVE_GATE))?;
        Some(lufs(second))
    }
}

/// Integrated loudness of a whole interleaved buffer.
#[must_use]
pub fn measure_lufs(interleaved: &[f32], rate: u32, channels: usize) -> Option<f64> {
    let mut m = LoudnessMeter::new(rate, channels);
    m.push(interleaved);
    m.integrated_lufs()
}

/// Gain in dB that moves `measured` LUFS to `target` LUFS.
#[must_use]
pub fn gain_to_target_db(measured: f64, target: f64) -> f64 {
    target - measured
}

/// The same as a linear factor (for the mixer's master gain). It reaches the target only if the
/// mixer's limiter does not act on the result; limiting takes level away afterwards.
#[must_use]
pub fn gain_to_target(measured: f64, target: f64) -> f32 {
    10f64.powf(gain_to_target_db(measured, target) / 20.0) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tol: f64) {
        assert!((a - b).abs() < tol, "{a} vs {b}");
    }

    #[test]
    fn designed_filters_match_the_standard_at_48k() {
        let [shelf, hp] = k_weighting(48000.0);
        close(shelf.b[0], 1.535_124_859_586_97, 1e-9);
        close(shelf.b[1], -2.691_696_189_406_38, 1e-9);
        close(shelf.b[2], 1.198_392_810_852_85, 1e-9);
        close(shelf.a[0], -1.690_659_293_182_41, 1e-9);
        close(shelf.a[1], 0.732_480_774_215_85, 1e-9);
        close(hp.a[0], -1.990_047_454_833_98, 1e-9);
        close(hp.a[1], 0.990_072_250_366_21, 1e-9);
    }

    #[test]
    fn target_gain_arithmetic() {
        close(gain_to_target_db(-20.0, SOCIAL_TARGET_LUFS), 6.0, 1e-12);
        close(f64::from(gain_to_target(-20.0, -14.0)), 1.995_262_3, 1e-6);
    }

    #[test]
    fn silence_has_no_loudness() {
        assert_eq!(measure_lufs(&vec![0.0; 48000 * 2 * 2], 48000, 2), None);
        assert_eq!(measure_lufs(&[0.1; 100], 48000, 2), None);
        // Below the absolute gate (-70 LUFS) there is nothing to report, however steady.
        let quiet: Vec<f32> = (0..48000 * 2 * 3)
            .map(|i| 3e-5 * ((i / 2) as f32 * 0.13).sin())
            .collect();
        assert_eq!(measure_lufs(&quiet, 48000, 2), None);
        // The second gate is the higher of -70 LUFS and 10 LU under the first mean: 10 s at -62
        // LUFS and 10 s at -71 LUFS (under the absolute gate) must read -62, not -64.5.
        let tone = |db: f64, secs: usize| -> Vec<f32> {
            (0..48000 * secs)
                .flat_map(|i| {
                    let v = (10f64.powf(db / 20.0)
                        * (std::f64::consts::TAU * 997.0 * i as f64 / 48000.0).sin())
                        as f32;
                    [v, v]
                })
                .collect()
        };
        let mut sig = tone(-62.0, 10);
        sig.extend(tone(-71.0, 10));
        close(measure_lufs(&sig, 48000, 2).unwrap(), -62.0, 0.2);
    }
}
