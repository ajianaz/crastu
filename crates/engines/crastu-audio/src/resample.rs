use std::f64::consts::PI;

use crate::error::AudioError;

/// The highest sample rate any entry point accepts. It bounds the kernel length (at most
/// about 770 taps per phase when converting to 16 kHz), so a corrupt header cannot make the
/// resampler allocate or compute without limit.
pub const MAX_RATE: u32 = 384_000;

/// Rejects rates outside `1..=`[`MAX_RATE`].
pub fn validate_rate(rate: u32) -> Result<(), AudioError> {
    if (1..=MAX_RATE).contains(&rate) {
        Ok(())
    } else {
        Err(AudioError::BadRate(rate))
    }
}

/// Zero crossings of the sinc kernel on each side at full bandwidth; the kernel is stretched
/// when downsampling so that it also low-passes below the new Nyquist frequency.
const TAPS: f64 = 16.0;

/// The kernel tables of one resampler (one per phase) may take this many bytes in all. A pair of
/// rates whose tables would be larger is refused, so no output sample ever needs its kernel
/// computed on the spot: every kernel is built at most once. (44.1 kHz to 16 kHz needs 160
/// tables of 192 floats.)
const MAX_CACHE_BYTES: u64 = 8 << 20;

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// A streaming windowed-sinc (Blackman) resampler for mono samples. Output sample `n` sits at
/// input time `n * in_rate / out_rate` exactly (integer arithmetic), so there is no drift, and
/// the kernel is normalised per phase so a constant stays constant. The signal is zero before
/// the first sample; [`Resampler::finish`] pads zeros after the last one.
#[derive(Debug)]
pub struct Resampler {
    in_rate: u64,
    out_rate: u64,
    step: u64,
    half: usize,
    cutoff: f64,
    buf: Vec<f32>,
    /// Absolute input index of `buf[0]`.
    base: i64,
    next_out: u64,
    total_in: u64,
    cache: Vec<Option<Box<[f32]>>>,
    /// Kernels computed so far (at most one per phase); the work counter tests look at.
    kernels_built: usize,
}

impl Resampler {
    /// Fails with [`AudioError::BadRate`] (carrying the input rate) for a rate outside
    /// `1..=`[`MAX_RATE`], or for a pair of rates whose kernel tables would be over the budget
    /// (odd rates far above 16 kHz, such as 383999 Hz).
    pub fn new(in_rate: u32, out_rate: u32) -> Result<Self, AudioError> {
        validate_rate(in_rate)?;
        validate_rate(out_rate)?;
        let (i, o) = (u64::from(in_rate), u64::from(out_rate));
        let cutoff = (o as f64 / i as f64).min(1.0);
        let g = gcd(i, o);
        let phases = o / g;
        let half = (TAPS / cutoff).ceil() as usize;
        if phases * 2 * half as u64 * 4 > MAX_CACHE_BYTES {
            return Err(AudioError::BadRate(in_rate));
        }
        Ok(Resampler {
            in_rate: i,
            out_rate: o,
            step: g,
            half,
            cutoff,
            buf: Vec::new(),
            base: 0,
            next_out: 0,
            total_in: 0,
            cache: vec![None; phases as usize],
            kernels_built: 0,
        })
    }

    pub fn is_passthrough(&self) -> bool {
        self.in_rate == self.out_rate
    }

    fn weights(&self, frac: f64) -> Box<[f32]> {
        let half = self.half as i64;
        let mut w: Vec<f64> = (0..2 * half)
            .map(|j| {
                let d = frac + (half - 1 - j) as f64;
                let u = d / self.half as f64;
                if u.abs() >= 1.0 {
                    return 0.0;
                }
                let x = self.cutoff * d;
                let sinc = if x.abs() < 1e-12 {
                    1.0
                } else {
                    (PI * x).sin() / (PI * x)
                };
                let win = 0.42 + 0.5 * (PI * u).cos() + 0.08 * (2.0 * PI * u).cos();
                sinc * win
            })
            .collect();
        let sum: f64 = w.iter().sum();
        for v in &mut w {
            *v /= sum;
        }
        w.into_iter().map(|v| v as f32).collect()
    }

    /// Feeds input samples and appends every output sample that is complete.
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        if self.is_passthrough() {
            out.extend_from_slice(input);
            return;
        }
        self.total_in += input.len() as u64;
        self.buf.extend_from_slice(input);
        self.run(out, u64::MAX);
    }

    /// Ends the stream: pads zeros and appends the remaining samples, so the total output is
    /// `ceil(total_in * out_rate / in_rate)`.
    pub fn finish(&mut self, out: &mut Vec<f32>) {
        if self.is_passthrough() {
            return;
        }
        let want = (self.total_in * self.out_rate).div_ceil(self.in_rate);
        self.buf.extend(std::iter::repeat_n(0.0, self.half + 1));
        self.run(out, want);
    }

    fn run(&mut self, out: &mut Vec<f32>, limit: u64) {
        let half = self.half as i64;
        let end = self.base + self.buf.len() as i64;
        while self.next_out < limit {
            let num = self.next_out * self.in_rate;
            let i0 = (num / self.out_rate) as i64;
            if i0 + half >= end {
                break;
            }
            let rem = num % self.out_rate;
            let first = i0 - half + 1;
            let mut acc = 0.0f32;
            let slot = (rem / self.step) as usize;
            if self.cache[slot].is_none() {
                self.cache[slot] = Some(self.weights(rem as f64 / self.out_rate as f64));
                self.kernels_built += 1;
            }
            let w: &[f32] = self.cache[slot].as_deref().unwrap_or(&[]);
            for (j, wj) in w.iter().enumerate() {
                let k = first + j as i64;
                if k >= self.base {
                    acc += wj * self.buf[(k - self.base) as usize];
                }
            }
            out.push(acc);
            self.next_out += 1;
        }
        // Drop the input no later output needs.
        let need = (self.next_out * self.in_rate / self.out_rate) as i64 - half + 1;
        if need > self.base {
            let drop = ((need - self.base) as usize).min(self.buf.len());
            self.buf.drain(..drop);
            self.base += drop as i64;
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn sine(freq: f64, rate: u32, secs: f64, amp: f32) -> Vec<f32> {
        let n = (f64::from(rate) * secs) as usize;
        (0..n)
            .map(|i| amp * (2.0 * PI * freq * i as f64 / f64::from(rate)).sin() as f32)
            .collect()
    }

    fn resample_all(x: &[f32], i: u32, o: u32, chunk: usize) -> Vec<f32> {
        let mut r = Resampler::new(i, o).unwrap();
        let mut out = Vec::new();
        for c in x.chunks(chunk) {
            r.process(c, &mut out);
        }
        r.finish(&mut out);
        out
    }

    fn zero_crossings(x: &[f32]) -> usize {
        x.windows(2)
            .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
            .count()
    }

    fn rms(x: &[f32]) -> f64 {
        (x.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>() / x.len() as f64).sqrt()
    }

    fn check_sine(rate: u32) {
        let x = sine(440.0, rate, 1.0, 0.5);
        let y = resample_all(&x, rate, 16_000, 1000);
        assert_eq!(y.len(), 16_000);
        // Skip the edges (the signal is not really zero before the start).
        let mid = &y[400..y.len() - 400];
        let secs = mid.len() as f64 / 16_000.0;
        let freq = zero_crossings(mid) as f64 / 2.0 / secs;
        assert!((freq - 440.0).abs() < 3.0, "{rate}: frequency {freq}");
        let want = 0.5 / 2f64.sqrt();
        let got = rms(mid);
        assert!(
            (got - want).abs() / want < 0.01,
            "{rate}: rms {got} vs {want}"
        );
    }

    #[test]
    fn sine_from_48k_keeps_frequency_and_amplitude() {
        check_sine(48_000);
    }

    #[test]
    fn sine_from_44k1_keeps_frequency_and_amplitude() {
        check_sine(44_100);
    }

    #[test]
    fn output_length_is_exact() {
        for (i, o, n) in [
            (48_000u32, 16_000u32, 4800usize),
            (44_100, 16_000, 4410),
            (8_000, 16_000, 801),
        ] {
            let y = resample_all(&vec![0.1; n], i, o, 333);
            assert_eq!(
                y.len() as u64,
                (n as u64 * u64::from(o)).div_ceil(u64::from(i))
            );
        }
    }

    #[test]
    fn chunking_does_not_change_the_result() {
        let x = sine(300.0, 44_100, 0.5, 0.7);
        let a = resample_all(&x, 44_100, 16_000, 1);
        let b = resample_all(&x, 44_100, 16_000, x.len());
        assert_eq!(a.len(), b.len());
        for (p, q) in a.iter().zip(&b) {
            assert!((p - q).abs() < 1e-6);
        }
    }

    #[test]
    fn tone_above_the_new_nyquist_is_removed() {
        // 12 kHz at 48 kHz is above 8 kHz: it must not alias into the 16 kHz output.
        let x = sine(12_000.0, 48_000, 0.5, 0.5);
        let y = resample_all(&x, 48_000, 16_000, 480);
        let mid = &y[200..y.len() - 200];
        assert!(rms(mid) < 0.01, "aliased rms {}", rms(mid));
    }

    #[test]
    fn dc_stays_dc_and_upsampling_works() {
        let y = resample_all(&vec![0.25; 4000], 8_000, 16_000, 500);
        assert_eq!(y.len(), 8000);
        assert!(y[100..7900].iter().all(|v| (v - 0.25).abs() < 1e-3));
    }

    fn gain_db(freq: f64) -> f64 {
        let x = sine(freq, 48_000, 0.5, 0.5);
        let y = resample_all(&x, 48_000, 16_000, 480);
        let mid = &y[200..y.len() - 200];
        20.0 * (rms(mid) / (0.5 / 2f64.sqrt())).log10()
    }

    #[test]
    fn frequency_response_48k_to_16k() {
        for f in [300.0, 1_000.0, 4_000.0, 6_000.0] {
            let g = gain_db(f);
            assert!(g.abs() < 0.2, "{f} Hz: {g} dB");
        }
        let g = gain_db(7_500.0);
        assert!((-3.0..0.2).contains(&g), "7.5 kHz: {g} dB");
        // Above 8 kHz the tone would alias into the band: it must be attenuated.
        let g = gain_db(8_500.0);
        assert!(g < -12.0, "8.5 kHz: {g} dB");
        let g = gain_db(9_000.0);
        assert!(g < -30.0, "9 kHz: {g} dB");
        let g = gain_db(10_000.0);
        assert!(g < -70.0, "10 kHz: {g} dB");
    }

    #[test]
    fn rates_outside_the_range_are_errors_not_panics() {
        for bad in [0, MAX_RATE + 1, 2_000_000_000, u32::MAX] {
            assert_eq!(
                Resampler::new(bad, 16_000).unwrap_err(),
                AudioError::BadRate(bad)
            );
            assert_eq!(
                Resampler::new(16_000, bad).unwrap_err(),
                AudioError::BadRate(bad)
            );
        }
        assert!(Resampler::new(MAX_RATE, 16_000).is_ok());
        assert!(Resampler::new(1, 16_000).is_ok());
    }

    fn table_bytes(i: u32, o: u32) -> Option<u64> {
        Resampler::new(i, o)
            .ok()
            .map(|r| r.cache.len() as u64 * 2 * r.half as u64 * 4)
    }

    #[test]
    fn pathological_rate_pairs_are_refused() {
        assert_eq!(
            Resampler::new(383_999, 16_000).unwrap_err(),
            AudioError::BadRate(383_999)
        );
        assert!(Resampler::new(384_000, 16_000).is_ok()); // a whole ratio: one phase
        assert!(Resampler::new(44_100, 16_000).is_ok());
        assert!(Resampler::new(96_000, 16_000).is_ok());
    }

    #[test]
    fn the_worst_accepted_pair_builds_a_bounded_number_of_kernels() {
        // Search the accepted input rates for the one with the biggest tables.
        let mut worst = (0u32, 0u64);
        for i in (1..=MAX_RATE)
            .step_by(7)
            .chain([44_100, 48_000, 88_200, 192_000, 22_050])
        {
            if let Some(b) = table_bytes(i, 16_000) {
                assert!(b <= MAX_CACHE_BYTES);
                if b > worst.1 {
                    worst = (i, b);
                }
            }
        }
        assert!(
            worst.1 > MAX_CACHE_BYTES / 4,
            "the search should reach big tables: {worst:?}"
        );
        let mut r = Resampler::new(worst.0, 16_000).unwrap();
        let phases = r.cache.len();
        let mut out = Vec::new();
        r.process(&vec![0.25; worst.0 as usize], &mut out); // one second
        r.finish(&mut out);
        assert_eq!(out.len(), 16_000);
        // The work: at most one kernel per phase, never one per output sample.
        assert!(
            r.kernels_built <= phases,
            "{} kernels for {phases} phases",
            r.kernels_built
        );
        assert!(r.kernels_built <= out.len());
        assert!(
            out[200..out.len() - 200]
                .iter()
                .all(|v| (v - 0.25).abs() < 1e-3)
        );
    }

    #[test]
    fn equal_rates_pass_through() {
        let x = vec![0.1, 0.2, 0.3];
        assert_eq!(resample_all(&x, 16_000, 16_000, 2), x);
    }
}
