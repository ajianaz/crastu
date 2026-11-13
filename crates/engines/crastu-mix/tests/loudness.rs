mod common;

use common::*;
use crastu_mix::{
    Clip, MixConfig, Mixer, SOCIAL_TARGET_LUFS, Timeline, Track, gain_to_target, measure_lufs,
};

#[test]
fn gain_to_target_brings_a_bounce_to_the_social_preset() {
    let r = u64::from(RATE);
    let mk = |master: f32| {
        let n = 10 * RATE as usize;
        let t = Timeline {
            clips: vec![
                Clip::new(Track::Voice, 0, 0, 10 * r),
                Clip::new(Track::Music, 1, 0, 10 * r),
            ],
            ..Timeline::default()
        };
        let sources = vec![
            mem(1, sine_mono(997.0, 0.05, n, RATE)),
            mem(2, sine_stereo(220.0, 0.04, n, RATE)),
        ];
        Mixer::new(
            t,
            sources,
            &MixConfig {
                master_gain: master,
                ..MixConfig::default()
            },
        )
        .unwrap()
    };
    let first = mk(1.0).bounce(0..10 * r);
    let measured = measure_lufs(&first, RATE, 2).unwrap();
    assert!(measured < -25.0, "starts quiet: {measured}");
    let gain = gain_to_target(measured, SOCIAL_TARGET_LUFS);
    let second = mk(gain).bounce(0..10 * r);
    let after = measure_lufs(&second, RATE, 2).unwrap();
    assert!((after - SOCIAL_TARGET_LUFS).abs() < 0.1, "{after}");
}
