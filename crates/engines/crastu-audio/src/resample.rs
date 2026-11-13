use std::f64::consts::PI;

/// Zero crossings of the sinc kernel on each side at full bandwidth; the kernel is stretched
/// when downsampling so that it also low-passes below the new Nyquist frequency.
const TAPS: f64 = 16.0;

/// Phase tables up to this many entries are cached (44.1 kHz to 16 kHz needs 160).
const MAX_CACHED_PHASES: u64 = 4096;

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
}

impl Resampler {
    /// # Panics
    /// If either rate is zero.
    pub fn new(in_rate: u32, out_rate: u32) -> Self {
        assert!(in_rate > 0 && out_rate > 0, "sample rates must be positive");
        let (i, o) = (u64::from(in_rate), u64::from(out_rate));
        let cutoff = (o as f64 / i as f64).min(1.0);
        let g = gcd(i, o);
        let phases = o / g;
        Resampler {
            in_rate: i,
            out_rate: o,
            step: g,
            half: (TAPS / cutoff).ceil() as usize,
            cutoff,
            buf: Vec::new(),
            base: 0,
            next_out: 0,
            total_in: 0,
            cache: if phases <= MAX_CACHED_PHASES {
                vec![None; phases as usize]
            } else {
                Vec::new()
            },
        }
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
            let computed;
            let w: &[f32] = if self.cache.is_empty() {
                computed = self.weights(rem as f64 / self.out_rate as f64);
                &computed
            } else {
                let slot = (rem / self.step) as usize;
                if self.cache[slot].is_none() {
                    self.cache[slot] = Some(self.weights(rem as f64 / self.out_rate as f64));
                }
                self.cache[slot].as_deref().unwrap_or(&[])
            };
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
        let mut r = Resampler::new(i, o);
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

    #[test]
    fn equal_rates_pass_through() {
        let x = vec![0.1, 0.2, 0.3];
        assert_eq!(resample_all(&x, 16_000, 16_000, 2), x);
    }
}
