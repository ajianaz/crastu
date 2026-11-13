use std::f64::consts::TAU;

use crastu_loudness::measure_lufs;

/// Mono sine, `amp` peak.
fn sine_mono(freq: f64, amp: f32, frames: usize, rate: u32) -> Vec<f32> {
    (0..frames)
        .map(|i| amp * (TAU * freq * i as f64 / f64::from(rate)).sin() as f32)
        .collect()
}

/// Deterministic white noise in -1..1.
struct Lcg(u64);

impl Lcg {
    fn next_f32(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 40) as f32 / (1u64 << 23) as f32) - 1.0
    }
}

fn stereo_of(mono: &[f32]) -> Vec<f32> {
    mono.iter().flat_map(|v| [*v, *v]).collect()
}

fn db(db: f64) -> f32 {
    10f64.powf(db / 20.0) as f32
}

#[test]
fn a_997_hz_stereo_sine_reads_its_level_in_lufs() {
    // BS.1770 is calibrated so that a stereo 997 Hz sine of peak level L dBFS reads L LUFS.
    for rate in [48_000u32, 44_100] {
        let tone = stereo_of(&sine_mono(997.0, db(-20.0), rate as usize * 10, rate));
        let lufs = measure_lufs(&tone, rate, 2).unwrap();
        assert!((lufs + 20.0).abs() < 0.1, "{rate} Hz: {lufs}");
    }
    let tone = stereo_of(&sine_mono(997.0, db(-23.0), 48_000 * 10, 48_000));
    assert!((measure_lufs(&tone, 48_000, 2).unwrap() + 23.0).abs() < 0.1);
}

/// Straight-line reference: the published 48 kHz coefficients in f64, mean square over the whole
/// signal (no gating), for stationary noise where gating removes nothing.
fn reference_lufs(interleaved: &[f32]) -> f64 {
    let stages = [
        (
            [1.53512485958697, -2.69169618940638, 1.19839281085285],
            [-1.69065929318241, 0.73248077421585],
        ),
        ([1.0, -2.0, 1.0], [-1.99004745483398, 0.99007225036621]),
    ];
    let mut total = 0.0;
    let mut count = 0usize;
    for c in 0..2 {
        let mut hist = [[0.0f64; 4]; 2]; // x1 x2 y1 y2 per stage
        for f in interleaved.as_chunks::<2>().0 {
            let mut v = f64::from(f[c]);
            for (st, h) in stages.iter().zip(hist.iter_mut()) {
                let y =
                    st.0[0] * v + st.0[1] * h[0] + st.0[2] * h[1] - st.1[0] * h[2] - st.1[1] * h[3];
                *h = [v, h[0], y, h[2]];
                v = y;
            }
            total += v * v;
        }
        count = interleaved.len() / 2;
    }
    -0.691 + 10.0 * (total / count as f64).log10()
}

/// Pink noise (Paul Kellet's filter) scaled to a peak of about `amp`.
fn pink(frames: usize, amp: f32, seed: u64) -> Vec<f32> {
    let mut rng = Lcg(seed);
    let (mut b0, mut b1, mut b2, mut b3, mut b4, mut b5, mut b6) =
        (0f32, 0f32, 0f32, 0f32, 0f32, 0f32, 0f32);
    let raw: Vec<f32> = (0..frames)
        .map(|_| {
            let w = rng.next_f32();
            b0 = 0.99886 * b0 + w * 0.0555179;
            b1 = 0.99332 * b1 + w * 0.0750759;
            b2 = 0.96900 * b2 + w * 0.153_852;
            b3 = 0.86650 * b3 + w * 0.3104856;
            b4 = 0.55000 * b4 + w * 0.5329522;
            b5 = -0.7616 * b5 - w * 0.0168980;
            let s = b0 + b1 + b2 + b3 + b4 + b5 + b6 + w * 0.5362;
            b6 = w * 0.115926;
            s
        })
        .collect();
    let peak = raw.iter().fold(0.0f32, |a, v| a.max(v.abs()));
    raw.iter().map(|v| v / peak * amp).collect()
}

#[test]
fn pink_noise_matches_the_straight_reference() {
    let noise = pink(48_000 * 20, 0.5, 11);
    let mut stereo = Vec::new();
    for (i, v) in noise.iter().enumerate() {
        stereo.extend_from_slice(&[*v, noise[(i + 977) % noise.len()]]);
    }
    let got = measure_lufs(&stereo, 48_000, 2).unwrap();
    let want = reference_lufs(&stereo);
    assert!((got - want).abs() < 0.1, "meter {got}, reference {want}");
}

#[test]
fn the_gates_drop_quiet_passages() {
    let rate = 48_000usize;
    let mut sig = stereo_of(&sine_mono(997.0, db(-20.0), rate * 6, 48_000));
    sig.extend(stereo_of(&sine_mono(997.0, db(-45.0), rate * 6, 48_000))); // 25 LU lower: relative gate
    sig.extend(stereo_of(&sine_mono(997.0, db(-80.0), rate * 6, 48_000))); // under -70: absolute gate
    let lufs = measure_lufs(&sig, 48_000, 2).unwrap();
    // The 400 ms blocks that straddle a change are partly loud and stay in, so the value is a
    // little under -20; without the gates the mean power would give about -23.
    assert!((lufs + 20.0).abs() < 0.2, "{lufs}");
}
