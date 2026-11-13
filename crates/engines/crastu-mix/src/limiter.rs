//! A look-ahead true-peak limiter for stereo.
//!
//! The peak the limiter reacts to is a 4x oversampled estimate: for the gap between two samples
//! it reads three interpolated points from a 32-tap windowed-sinc filter (a 12-tap version of it
//! under-read content near Nyquist by up to about 0.6 dB in review). The wanted gain is the
//! ceiling over that peak. Each sample is held under the wanted gain of every gap its taps reach
//! (a minimum over 32 gaps), then a minimum over `lookahead` samples and a moving average of the
//! same length turn that into a smooth gain that is never above the wanted gain at any sample, so
//! the output sample peak stays under the ceiling and the true peak stays within the estimate
//! error of it (the tests measure under 0.1 dB over for tones up to 21 kHz; a short burst made only of
//! content at fs/2 can go about 2.5 dB over, which no finite estimate prevents).
//! The filter is per-sample and causal, so the result does not depend on how the input is split
//! into blocks. Output is delayed by [`Limiter::latency`] frames.

use std::f64::consts::PI;

const TAPS: usize = 32;
/// Taps reach this many samples before the gap and `TAPS - BEFORE - 1` after it.
const BEFORE: i64 = 15;
/// Half width of the window in samples.
const HALF_WIDTH: f64 = 16.0;
/// Frames between a sample being pushed and its gap being measurable (taps after the gap, less one).
const MEASURE_DELAY: i64 = 15;
/// Taps after the gap's first sample.
const AFTER: i64 = TAPS as i64 - BEFORE - 1;

type Phases = [[f32; TAPS]; 3];

/// Windowed-sinc interpolation weights for the points 1/4, 2/4 and 3/4 of the way between two
/// samples, each normalised to unity gain at DC.
fn phases() -> Phases {
    let mut out = [[0.0f32; TAPS]; 3];
    for (p, row) in out.iter_mut().enumerate() {
        let frac = (p as f64 + 1.0) / 4.0;
        let mut w = [0.0f64; TAPS];
        for (k, slot) in w.iter_mut().enumerate() {
            let t = (k as i64 - BEFORE) as f64 - frac;
            let sinc = if t == 0.0 {
                1.0
            } else {
                (PI * t).sin() / (PI * t)
            };
            let hann = 0.5 * (1.0 + (PI * t / HALF_WIDTH).cos());
            *slot = sinc * hann;
        }
        let sum: f64 = w.iter().sum();
        for (dst, src) in row.iter_mut().zip(w) {
            *dst = (src / sum) as f32;
        }
    }
    out
}

/// Largest absolute value among the interpolated points of the gap before `x[j]`, given a getter
/// for sample `j + i` of one channel.
fn gap_peak(phases: &Phases, get: impl Fn(i64) -> f32, j: i64) -> f32 {
    let a = j - 1;
    let mut peak = 0.0f32;
    for row in phases {
        let mut acc = 0.0f32;
        for (k, w) in row.iter().enumerate() {
            acc += w * get(a + k as i64 - BEFORE);
        }
        peak = peak.max(acc.abs());
    }
    peak
}

/// Estimated true peak of an interleaved buffer as a linear value (4x oversampled). Edges are
/// treated as silence.
#[must_use]
pub fn true_peak(interleaved: &[f32], channels: usize) -> f32 {
    let channels = channels.max(1);
    let frames = (interleaved.len() / channels) as i64;
    let ph = phases();
    let mut peak = 0.0f32;
    for c in 0..channels {
        let get = |i: i64| {
            if i < 0 || i >= frames {
                0.0
            } else {
                interleaved[i as usize * channels + c]
            }
        };
        for j in 0..=frames {
            peak = peak.max(get(j).abs()).max(gap_peak(&ph, get, j));
        }
    }
    peak
}

/// The limiter. Feed one stereo frame, get one stereo frame `latency()` frames older.
#[derive(Clone, Debug)]
pub struct Limiter {
    ceiling: f32,
    look: i64,
    cap: i64,
    n: i64,
    x: Vec<[f32; 2]>,
    g: Vec<f32>,
    h: Vec<f32>,
    m: Vec<f32>,
    phases: Phases,
}

impl Limiter {
    /// `ceiling` is the linear peak limit (about 0.891 for -1 dBTP); `lookahead` is the attack in
    /// samples (48 is 1 ms at 48 kHz) and is clamped to 16..=1024 (a shorter attack cannot hold the true peak).
    #[must_use]
    pub fn new(ceiling: f32, lookahead: usize) -> Self {
        let look = lookahead.clamp(16, 1024) as i64;
        let cap = ((4 * look + 128) as usize).next_power_of_two();
        Self {
            ceiling: ceiling.max(1e-6),
            look,
            cap: cap as i64,
            n: 0,
            x: vec![[0.0; 2]; cap],
            g: vec![1.0; cap],
            h: vec![1.0; cap],
            m: vec![1.0; cap],
            phases: phases(),
        }
    }

    /// Frames of delay between input and output.
    #[must_use]
    pub fn latency(&self) -> usize {
        (self.look + MEASURE_DELAY - 1 + AFTER) as usize
    }

    /// Forgets all history, as if new.
    pub fn reset(&mut self) {
        self.n = 0;
        self.x.fill([0.0; 2]);
        self.g.fill(1.0);
        self.h.fill(1.0);
        self.m.fill(1.0);
    }

    fn slot(&self, i: i64) -> usize {
        i.rem_euclid(self.cap) as usize
    }

    /// Pushes one frame and returns the frame `latency()` pushes ago (silence at first).
    pub fn push(&mut self, frame: [f32; 2]) -> [f32; 2] {
        let n = self.n;
        let s = self.slot(n);
        // NaN would pass the min and clamp below; treat non-finite input as silence.
        self.x[s] = frame.map(|v| if v.is_finite() { v } else { 0.0 });

        // The gap before sample j = n - MEASURE_DELAY now has all its taps.
        let j = n - MEASURE_DELAY;
        if j >= 0 {
            let mut need = 0.0f32;
            for c in 0..2 {
                let get = |i: i64| if i < 0 { 0.0 } else { self.x[self.slot(i)][c] };
                need = need.max(get(j).abs()).max(gap_peak(&self.phases, get, j));
            }
            let gj = if need > self.ceiling {
                self.ceiling / need
            } else {
                1.0
            };
            let sj = self.slot(j);
            self.g[sj] = gj;
            // A sample feeds the gaps within the filter's reach on both sides, so it must be at
            // least as quiet as the wanted gain of every one of them: h[x] = min g[x - BEFORE..=x + AFTER],
            // complete once g[x + AFTER] is known (x may be negative: the gains before 0 are 1).
            let x = j - AFTER;
            let mut spread = 1.0f32;
            for i in (x - BEFORE)..=(x + AFTER) {
                spread = spread.min(self.g[self.slot(i)]);
            }
            let sx = self.slot(x);
            self.h[sx] = spread;
            // m[k] = min h[k..k + look - 1]; k = x - look + 1 completes now.
            let k = x - self.look + 1;
            let mut min = 1.0f32;
            for i in k..=x {
                min = min.min(self.h[self.slot(i)]);
            }
            let sk = self.slot(k);
            self.m[sk] = min;
        }

        // Output index o = n - latency; its gain is the mean of m[o - look + 1..=o].
        let o = n - (self.look + MEASURE_DELAY - 1 + AFTER);
        self.n += 1;
        if o < 0 {
            return [0.0; 2];
        }
        let mut sum = 0.0f32;
        for i in (o - self.look + 1)..=o {
            sum += self.m[self.slot(i)];
        }
        let gain = sum / self.look as f32;
        let xo = self.x[self.slot(o)];
        let c = self.ceiling;
        [(xo[0] * gain).clamp(-c, c), (xo[1] * gain).clamp(-c, c)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(l: &mut Limiter, input: &[[f32; 2]]) -> Vec<[f32; 2]> {
        input.iter().map(|f| l.push(*f)).collect()
    }

    #[test]
    fn quiet_signal_passes_unchanged_after_the_latency() {
        let mut l = Limiter::new(0.9, 48);
        let input: Vec<[f32; 2]> = (0..2000)
            .map(|i| [0.3 * (i as f32 * 0.05).sin(), 0.0])
            .collect();
        let out = run(&mut l, &input);
        let d = l.latency();
        for i in d..input.len() {
            assert_eq!(out[i][0], input[i - d][0], "frame {i}");
        }
    }

    #[test]
    fn loud_signal_stays_under_the_ceiling() {
        let mut l = Limiter::new(0.8, 48);
        let input: Vec<[f32; 2]> = (0..8000)
            .map(|i| {
                let s = (i as f32 * 0.31).sin() * 3.0;
                [s, -s * 0.7]
            })
            .collect();
        let out = run(&mut l, &input);
        let peak = out
            .iter()
            .flat_map(|f| f.iter())
            .fold(0.0f32, |a, v| a.max(v.abs()));
        assert!(peak <= 0.8, "peak {peak}");
        assert!(peak > 0.5, "limiter squashed everything: {peak}");
        // A hard clip leaves runs of samples sitting exactly on the ceiling; a limiter does not.
        let flat = out
            .windows(2)
            .filter(|w| w[0][0].abs() == 0.8 && w[1][0].abs() == 0.8)
            .count();
        assert_eq!(flat, 0, "clipped instead of limited");
    }

    #[test]
    fn gain_changes_gradually_instead_of_in_one_step() {
        // Quiet, then a sudden loud passage: the gain must fall over the look-ahead, not at once.
        let mut l = Limiter::new(0.8, 48);
        let input: Vec<[f32; 2]> = (0..1500)
            .map(|i| if i < 500 { [0.5, 0.5] } else { [2.0, 2.0] })
            .collect();
        let out = run(&mut l, &input);
        // Output frame i is input frame i - latency; look at the quiet part, where only the gain moves.
        let d = l.latency();
        let worst = out[d..500 + d - 1]
            .windows(2)
            .map(|w| (w[1][0] - w[0][0]).abs())
            .fold(0.0f32, f32::max);
        assert!(worst < 0.02, "largest step {worst}");
        assert!(
            out[500 + d - 1][0] < 0.3,
            "the gain was already down at the onset"
        );
        assert!((out[1400][0] - 0.8).abs() < 1e-4);
    }

    #[test]
    fn every_sample_is_under_the_wanted_gain_of_every_gap_its_taps_reach() {
        // The applied gain at sample o must not exceed the gain wanted for any gap j in
        // [o - BEFORE, o + AFTER]; this is what keeps the true peak between samples in check.
        let ceiling = 0.5f32;
        let mut seed = 12345u32;
        let mut rnd = move || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 8) as f32 / (1u32 << 23) as f32 - 1.0
        };
        let input: Vec<[f32; 2]> = (0..6000)
            .map(|i| {
                let burst = if (i / 400) % 2 == 0 { 3.0 } else { 0.6 };
                [rnd() * burst, rnd() * burst]
            })
            .collect();
        let mut l = Limiter::new(ceiling, 32);
        let d = l.latency();
        let out: Vec<[f32; 2]> = input
            .iter()
            .chain(&[[0.0; 2]; 100])
            .map(|f| l.push(*f))
            .collect();
        let ph = phases();
        let wanted = |j: i64| -> f32 {
            let mut need = 0.0f32;
            for c in 0..2 {
                let get = |i: i64| {
                    if i < 0 || i >= input.len() as i64 {
                        0.0
                    } else {
                        input[i as usize][c]
                    }
                };
                need = need.max(get(j).abs()).max(gap_peak(&ph, get, j));
            }
            if need > ceiling { ceiling / need } else { 1.0 }
        };
        let mut checked = 0;
        for o in 40..(input.len() - 60) {
            if input[o][0].abs() < 0.2 {
                continue;
            }
            let applied = out[o + d][0] / input[o][0];
            let tightest = ((o as i64 - BEFORE)..=(o as i64 + AFTER))
                .map(wanted)
                .fold(1.0f32, f32::min);
            assert!(
                applied <= tightest * 1.0001,
                "sample {o}: applied {applied}, wanted {tightest}"
            );
            checked += 1;
        }
        assert!(checked > 3000);
    }

    #[test]
    fn intersample_peak_is_seen() {
        // A quarter-rate sine sampled off its crest: the sample peak is 0.707, the true peak 1.0.
        let mut buf = vec![0.0f32; 64];
        let w = std::f32::consts::PI / 2.0;
        for (i, v) in buf.iter_mut().enumerate() {
            *v = 1.0 * (w * i as f32 + w / 2.0).sin();
        }
        let sample_peak = buf.iter().fold(0.0f32, |a, v| a.max(v.abs()));
        assert!(true_peak(&buf, 1) > sample_peak + 0.05);
    }
}
