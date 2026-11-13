use super::*;

const ALL: [Easing; 5] = [
    Easing::Linear,
    Easing::CubicIn,
    Easing::CubicOut,
    Easing::CubicInOut,
    Easing::BackOut { overshoot: 1.70158 },
];

#[test]
fn easing_endpoints() {
    for e in ALL {
        assert_eq!(e.at(0.0), 0.0, "{e:?}");
        assert_eq!(e.at(1.0), 1.0, "{e:?}");
        assert_eq!(e.at(-3.0), 0.0, "{e:?}");
        assert_eq!(e.at(9.0), 1.0, "{e:?}");
        assert_eq!(e.at(f64::NAN), 0.0, "{e:?}");
    }
}

#[test]
fn easing_known_values() {
    assert_eq!(Easing::Linear.at(0.25), 0.25);
    assert_eq!(Easing::CubicIn.at(0.5), 0.125);
    assert_eq!(Easing::CubicOut.at(0.5), 0.875);
    assert_eq!(Easing::CubicInOut.at(0.25), 0.0625);
    assert_eq!(Easing::CubicInOut.at(0.5), 0.5);
    assert_eq!(Easing::CubicInOut.at(0.75), 0.9375);
    let back = Easing::BackOut { overshoot: 1.70158 };
    // 1 + 2.70158 * (-0.5)^3 + 1.70158 * 0.25
    assert!(
        (back.at(0.5) - 1.087_697_5).abs() < 1e-6,
        "{}",
        back.at(0.5)
    );
    assert!(back.at(0.7) > 1.0, "overshoots");
}

#[test]
fn monotonic_easings_never_decrease() {
    for e in [
        Easing::Linear,
        Easing::CubicIn,
        Easing::CubicOut,
        Easing::CubicInOut,
    ] {
        let mut last = 0.0;
        for i in 0..=1000 {
            let v = e.at(f64::from(i) / 1000.0);
            assert!(v >= last, "{e:?} at {i}");
            last = v;
        }
    }
}

#[test]
fn spring_critical_damping_known_value() {
    // omega * t = 1 gives 1 - 2/e for a critically damped spring.
    let s = Spring::new(2.0 * PI, 0.0);
    assert!((s.at(1.0) - (1.0 - 2.0 / std::f64::consts::E)).abs() < 1e-12);
    assert_eq!(s.at(0.0), 0.0);
    assert_eq!(s.at(-1.0), 0.0);
    assert_eq!(s.at(s.settle_time()), 1.0);
    let mut last = 0.0;
    for i in 0..400 {
        let v = s.at(f64::from(i) * 0.05);
        assert!(v >= last && v <= 1.0);
        last = v;
    }
}

#[test]
fn spring_with_bounce_overshoots_then_settles() {
    let s = Spring::new(0.5, 0.5);
    let peak = (0..2000)
        .map(|i| s.at(f64::from(i) * 0.001))
        .fold(0.0_f64, f64::max);
    assert!(peak > 1.05, "peak {peak}");
    assert_eq!(s.at(10.0), 1.0);
    // The jump where the clamp starts is tiny.
    let t = s.settle_time();
    assert!((s.at(t - 1e-9) - 1.0).abs() < 1e-3);
}

#[test]
fn spring_degenerate_inputs_do_not_panic_or_nan() {
    for s in [
        Spring::new(0.0, 0.5),
        Spring::new(-1.0, 3.0),
        Spring::new(f64::NAN, f64::NAN),
    ] {
        for t in [-1.0, 0.0, 0.01, 1.0, f64::INFINITY, f64::NAN] {
            assert!(s.at(t).is_finite(), "{s:?} {t}");
        }
    }
}

#[test]
fn order_of_evaluation_does_not_change_bits() {
    let curves = [
        Curve::Ease {
            easing: Easing::CubicInOut,
            duration: 0.4,
        },
        Curve::Ease {
            easing: Easing::BackOut { overshoot: 2.0 },
            duration: 0.3,
        },
        Curve::Spring {
            spring: Spring::new(0.6, 0.35),
        },
        Curve::Spring {
            spring: Spring::new(0.6, 0.0),
        },
    ];
    let times: Vec<f64> = (0..200).map(|i| f64::from(i) * 0.0137).collect();
    for c in curves {
        let forward: Vec<u64> = times.iter().map(|&t| c.at(t).to_bits()).collect();
        let mut order: Vec<usize> = (0..times.len()).collect();
        order.sort_by_key(|&i| (i * 7919) % 211);
        for &i in &order {
            assert_eq!(
                c.at(times[i]).to_bits(),
                forward[i],
                "{c:?} at {}",
                times[i]
            );
        }
        for i in (0..times.len()).rev() {
            assert_eq!(c.at(times[i]).to_bits(), forward[i]);
        }
    }
}

#[test]
fn ease_curve_with_zero_duration_jumps() {
    let c = Curve::Ease {
        easing: Easing::Linear,
        duration: 0.0,
    };
    assert_eq!(c.at(-0.1), 0.0);
    assert_eq!(c.at(0.0), 1.0);
}

#[test]
fn infinite_time_is_the_end_of_the_curve() {
    for e in ALL {
        assert_eq!(e.at(f64::INFINITY), 1.0, "{e:?}");
        assert_eq!(e.at(f64::NEG_INFINITY), 0.0, "{e:?}");
    }
    assert_eq!(Spring::new(0.5, 0.3).at(f64::INFINITY), 1.0);
    let c = Curve::Ease {
        easing: Easing::Linear,
        duration: 1e-320,
    };
    assert_eq!(c.at(1.0), 1.0);
}
