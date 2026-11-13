mod common;

use common::*;
use crastu_mix::{Automation, Clip, MixConfig, Mixer, Timeline, Track, db_to_gain, true_peak};

/// Renders `frames` frames from sample 0 with the given block sizes (cycled).
fn render_with(mixer: &mut Mixer, frames: usize, sizes: &mut dyn FnMut() -> usize) -> Vec<f32> {
    mixer.seek(0);
    let mut out = Vec::with_capacity(frames * 2);
    while out.len() < frames * 2 {
        let n = sizes().min(frames - out.len() / 2);
        let mut block = vec![0.0; n * 2];
        mixer.process(&mut block);
        out.extend_from_slice(&block);
    }
    out
}

#[test]
fn output_is_bit_identical_for_any_block_size() {
    let frames = scene_len() as usize;
    let reference = scene_mixer(1.0).bounce(0..frames as u64);
    assert!(reference.iter().any(|v| v.abs() > 0.1), "silent scene");
    for size in [1usize, 64, 480, 4096] {
        let mut m = scene_mixer(1.0);
        let got = render_with(&mut m, frames, &mut || size);
        assert_eq!(bits(&got), bits(&reference), "block size {size}");
    }
    let mut lcg = Lcg(7);
    let mut m = scene_mixer(1.0);
    let got = render_with(&mut m, frames, &mut || {
        1 + ((lcg.next_f32() + 1.0) * 1500.0) as usize
    });
    assert_eq!(bits(&got), bits(&reference), "random block sizes");
}

#[test]
fn limiter_engaged_scene_is_also_block_independent() {
    let frames = scene_len() as usize;
    let reference = scene_mixer(6.0).bounce(0..frames as u64);
    let peak = reference.iter().fold(0.0f32, |a, v| a.max(v.abs()));
    assert!(
        peak > 0.8,
        "limiter should be working near the ceiling, peak {peak}"
    );
    for size in [1usize, 333, 4096] {
        let mut m = scene_mixer(6.0);
        assert_eq!(
            bits(&render_with(&mut m, frames, &mut || size)),
            bits(&reference),
            "size {size}"
        );
    }
}

#[test]
fn live_tap_in_callback_blocks_equals_bounce_of_the_same_span() {
    let (from, to) = (50_000u64, 140_000u64);
    let reference = scene_mixer(1.0).bounce(from..to);
    for callback in [128usize, 480, 1024] {
        let mut live = scene_mixer(1.0);
        live.seek(from);
        let mut got = Vec::new();
        while live.position() < to {
            let n = callback.min((to - live.position()) as usize);
            let mut block = vec![0.0; n * 2];
            live.process(&mut block);
            got.extend_from_slice(&block);
        }
        assert_eq!(bits(&got), bits(&reference), "callback {callback}");
    }
}

#[test]
fn bounce_of_a_span_matches_the_slice_of_a_longer_bounce_below_the_ceiling() {
    let whole = scene_mixer(1.0).bounce(0..scene_len());
    let part = scene_mixer(1.0).bounce(20_000..90_000);
    assert_eq!(bits(&part), bits(&whole[40_000..180_000]));
}

#[test]
fn a_cut_lands_on_the_event_sample() {
    // A constant source, hard clip edges: the first and last audible samples are the event positions.
    let mk = || {
        let t = Timeline {
            clips: vec![Clip::new(Track::Music, 0, 1000, 500)],
            ..Timeline::default()
        };
        Mixer::new(t, vec![mem(1, vec![0.5; 4000])], &MixConfig::default()).unwrap()
    };
    let off = mk().bounce(0..2000);
    let first = off.chunks(2).position(|f| f[0] != 0.0).unwrap();
    let last = off.chunks(2).rposition(|f| f[0] != 0.0).unwrap();
    assert_eq!(
        first, 1001,
        "sample 1000 is the start of the fade, whose first gain is zero"
    );
    assert_eq!(
        last, 1498,
        "sample 1499 is the end of the fade, whose last gain is zero"
    );
}

#[test]
fn no_clicks_at_cuts_fades_and_ducking() {
    let n = 3 * RATE as usize;
    let sources = vec![
        mem(1, sine_mono(300.0, 0.5, n, RATE)),
        mem(2, sine_stereo(200.0, 0.4, n, RATE)),
    ];
    let r = u64::from(RATE);
    let mut v = Clip::new(Track::Voice, 0, r / 2, r + 17);
    v.fade_in = 0; // only the built-in minimum fade
    v.source_offset = 777; // the cut lands mid-wave, not on a zero crossing
    let mut v2 = Clip::new(Track::Voice, 0, r * 2, r / 4 + 5);
    v2.source_offset = 1234;
    let mut m = Clip::new(Track::Music, 1, 100, r * 2 + 3333);
    m.source_offset = 411;
    let t = Timeline {
        clips: vec![v, v2, m],
        ..Timeline::default()
    };
    let out = Mixer::new(t, sources, &MixConfig::default())
        .unwrap()
        .bounce(0..3 * r);
    assert!(out.iter().any(|v| v.abs() > 0.3));
    let worst = max_jump(&out);
    assert!(worst < 0.08, "largest sample-to-sample jump {worst}");
}

#[test]
fn the_click_detector_does_catch_a_hard_cut() {
    let mut s = sine_stereo(300.0, 0.5, 4000, RATE);
    s[2000 * 2..].fill(0.0);
    assert!(max_jump(&s) > 0.08);
}

fn max_jump(interleaved: &[f32]) -> f32 {
    let mut worst = 0.0f32;
    for c in 0..2 {
        for i in 1..interleaved.len() / 2 {
            worst = worst.max((interleaved[i * 2 + c] - interleaved[(i - 1) * 2 + c]).abs());
        }
    }
    worst
}

#[test]
fn limiter_never_exceeds_the_ceiling_on_loud_input() {
    let cfg = MixConfig {
        master_gain: 8.0,
        ceiling_db: -1.0,
        ..MixConfig::default()
    };
    let (t, s, _) = scene(1.0);
    let out = Mixer::new(t, s, &cfg).unwrap().bounce(0..scene_len());
    let ceiling = db_to_gain(-1.0);
    let sample_peak = out.iter().fold(0.0f32, |a, v| a.max(v.abs()));
    assert!(
        sample_peak <= ceiling,
        "sample peak {sample_peak} over {ceiling}"
    );
    assert!(
        sample_peak > ceiling * 0.9,
        "limiter over-attenuates: {sample_peak}"
    );
    // The oversampled peak may pass the ceiling by the estimate's error only.
    let at_ceiling = out.iter().filter(|v| v.abs() >= ceiling * 0.9999).count();
    assert!(
        at_ceiling < out.len() / 100,
        "hard clipping: {at_ceiling} samples sit on the ceiling"
    );
    let tp = true_peak(&out, 2);
    assert!(tp <= ceiling * db_to_gain(0.25), "true peak {tp}");
}

#[test]
fn ducking_lowers_the_music_under_the_voice_and_restores_it() {
    let r = u64::from(RATE);
    let n = 4 * RATE as usize;
    let sources = vec![mem(1, vec![0.0; n]), mem(1, sine_mono(500.0, 0.4, n, RATE))];
    let t = Timeline {
        clips: vec![
            Clip::new(Track::Voice, 0, r + r / 2, r),
            Clip::new(Track::Music, 1, 0, 4 * r),
        ],
        ..Timeline::default()
    };
    let cfg = MixConfig {
        duck_db: -12.0,
        duck_attack_ms: 30.0,
        duck_release_ms: 400.0,
        ..MixConfig::default()
    };
    let out = Mixer::new(t, sources, &cfg).unwrap().bounce(0..4 * r);
    let rms = |a: u64, b: u64| {
        let s: f64 = (a..b).map(|i| f64::from(out[i as usize * 2]).powi(2)).sum();
        (s / (b - a) as f64).sqrt()
    };
    let free = rms(r / 2, r + r / 4);
    let ducked = rms(r + r / 2 + 2000, 2 * r + r / 2 - 2000);
    let back = rms(3 * r + r / 4, 4 * r - 2000);
    let ratio_db = 20.0 * (ducked / free).log10();
    assert!((ratio_db + 12.0).abs() < 0.3, "ducked by {ratio_db} dB");
    assert!((20.0 * (back / free).log10()).abs() < 0.3, "not restored");
}

#[test]
fn rejects_a_clip_with_a_missing_source() {
    let t = Timeline {
        clips: vec![Clip::new(Track::Voice, 3, 0, 10)],
        ..Timeline::default()
    };
    assert!(Mixer::new(t, vec![], &MixConfig::default()).is_err());
}

#[test]
fn a_clip_plays_its_source_from_the_offset_at_its_start() {
    let src: Vec<f32> = (0..20_000).map(|i| i as f32 * 1e-5).collect(); // 0.0 .. 0.2
    let mut c = Clip::new(Track::Music, 0, 1000, 5000);
    c.source_offset = 250;
    c.gain = 0.5;
    let t = Timeline {
        clips: vec![c],
        ..Timeline::default()
    };
    let out = Mixer::new(t, vec![mem(1, src.clone())], &MixConfig::default())
        .unwrap()
        .bounce(0..7000);
    for pos in [1500usize, 2000, 4000, 5900] {
        let want = src[250 + pos - 1000] * 0.5;
        assert_eq!(out[pos * 2], want, "pos {pos}");
        assert_eq!(out[pos * 2 + 1], want, "mono is played on both sides");
    }
    assert_eq!(out[6000 * 2], 0.0);
}

#[test]
fn odd_inputs_do_not_panic_or_leak_non_finite_values() {
    let mut bad = vec![0.3; 3000];
    bad[1500] = f32::NAN;
    bad[1501] = f32::INFINITY;
    let mut c = Clip::new(Track::Music, 0, 0, 3000);
    c.source_offset = u64::MAX - 10; // past the end of the source: silence, no overflow
    let mut c2 = Clip::new(Track::Voice, 0, 0, 3000);
    c2.fade_in = u64::MAX;
    let t = Timeline {
        clips: vec![c, c2, Clip::new(Track::Voice, 0, u64::MAX - 5, 100)],
        ..Timeline::default()
    };
    let mut m = Mixer::new(t, vec![mem(1, bad)], &MixConfig::default()).unwrap();
    let out = m.bounce(0..3000);
    assert!(out.iter().all(|v| v.is_finite()));
    let mut odd = vec![1.0; 7];
    m.seek(u64::MAX - 2);
    m.process(&mut odd);
    assert_eq!(odd[6], 0.0, "an odd last sample is zeroed");
    assert!(
        Mixer::new(Timeline::default(), vec![], &MixConfig::default())
            .unwrap()
            .bounce(0..10)
            .iter()
            .all(|v| *v == 0.0)
    );
}

#[test]
fn buses_channels_and_music_automation_are_routed_as_documented() {
    let pairs = |l: f32, r: f32, n: usize| -> Vec<f32> { (0..n).flat_map(|_| [l, r]).collect() };
    let sources = vec![
        mem(2, pairs(0.3, -0.1, 60_000)),
        mem(2, pairs(0.4, 0.2, 60_000)),
    ];
    let t = Timeline {
        clips: vec![
            Clip::new(Track::Music, 1, 0, 10_000),
            Clip::new(Track::Voice, 0, 40_000, 10_000),
        ],
        music_gain: Automation::new(vec![(0, 0.0), (10_000, 1.0)]),
    };
    let out = Mixer::new(t, sources, &MixConfig::default())
        .unwrap()
        .bounce(0..60_000);
    let at = |pos: usize| (out[pos * 2], out[pos * 2 + 1]);
    // Music: its own channels, scaled by the automation (0.5 halfway), not ducked.
    let (l, r) = at(5000);
    assert!(
        (l - 0.2).abs() < 1e-6 && (r - 0.1).abs() < 1e-6,
        "music {l} {r}"
    );
    // Voice: its own channels, at full gain (not scaled by the music automation, not ducked).
    let (l, r) = at(45_000);
    assert!(
        (l - 0.3).abs() < 1e-6 && (r + 0.1).abs() < 1e-6,
        "voice {l} {r}"
    );
}
