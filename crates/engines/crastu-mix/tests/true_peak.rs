use std::f64::consts::{PI, TAU};

use crastu_mix::{Limiter, db_to_gain};

/// Independent estimate: 16x oversampling with a 257-tap Hann-windowed sinc, over the middle of
/// a stereo buffer.
#[allow(clippy::needless_range_loop)]
fn reference_true_peak(x: &[[f32; 2]], from: usize, to: usize) -> f32 {
    let mut peak = 0.0f64;
    for c in 0..2 {
        for a in from..to {
            peak = peak.max(f64::from(x[a][c]).abs());
            for q in 1..16 {
                let f = f64::from(q) / 16.0;
                let mut acc = 0.0;
                for i in -128i64..=128 {
                    let t = i as f64 - f;
                    let sinc = (PI * t).sin() / (PI * t);
                    let w = 0.5 * (1.0 + (PI * t / 129.0).cos());
                    acc += f64::from(x[(a as i64 + i) as usize][c]) * sinc * w;
                }
                peak = peak.max(acc.abs());
            }
        }
    }
    peak as f32
}

fn limited(input: &[[f32; 2]], ceiling: f32) -> Vec<[f32; 2]> {
    let mut l = Limiter::new(ceiling, 48);
    let d = l.latency();
    let mut out: Vec<[f32; 2]> = input.iter().map(|f| l.push(*f)).collect();
    out.drain(..d);
    out
}

#[test]
fn true_peak_stays_close_to_the_ceiling_for_tones_up_to_21_khz() {
    let ceiling = db_to_gain(-1.0);
    for freq in [1_000.0, 12_000.0, 18_000.0, 21_000.0] {
        // Several phases: the worst case is a crest that falls between two samples.
        for phase in [0.0, 0.4, 0.8, 1.2, 1.6, 2.0, 2.4, 2.8] {
            let input: Vec<[f32; 2]> = (0..4000)
                .map(|i| {
                    let s = (TAU * freq * i as f64 / 48_000.0 + phase).sin() as f32;
                    [s * 1.5, s * 1.2]
                })
                .collect();
            let out = limited(&input, ceiling);
            let tp = reference_true_peak(&out, 1800, 2000);
            let over_db = 20.0 * f64::from(tp / ceiling).log10();
            assert!(over_db < 0.1, "{freq} Hz: {over_db:.2} dB over the ceiling");
        }
    }
}

#[test]
fn a_short_burst_at_nyquist_is_the_known_limit_of_the_estimate() {
    // Edge ringing of a 40-sample alternating burst (all its energy at fs/2) is the hard case for
    // any finite estimator: measured 2.5 dB over the ceiling. Real programme material is far
    // from this; the test pins the limit so a change that makes it worse is seen.
    let ceiling = db_to_gain(-1.0);
    let mut input = vec![[0.0f32; 2]; 1000];
    for (i, f) in input.iter_mut().enumerate().skip(250).take(40) {
        let s = if i % 2 == 0 { 1.5 } else { -1.5 };
        *f = [s, s];
    }
    let out = limited(&input, ceiling);
    let over_db = 20.0 * f64::from(reference_true_peak(&out, 150, 450) / ceiling).log10();
    assert!(over_db < 3.0, "{over_db:.2} dB over the ceiling");
}
