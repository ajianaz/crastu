use super::*;
use crate::de::Lab;

fn lab(l: f64, a: f64, b: f64) -> Lab {
    Lab { l, a, b }
}

#[test]
fn delta_e2000_matches_the_published_test_pairs() {
    // Sharma, Wu and Dalal, "The CIEDE2000 colour-difference formula", tables of test data.
    let pairs = [
        (
            lab(50.0, 2.6772, -79.7751),
            lab(50.0, 0.0, -82.7485),
            2.0425,
        ),
        (
            lab(50.0, 3.1571, -77.2803),
            lab(50.0, 0.0, -82.7485),
            2.8615,
        ),
        (
            lab(50.0, -1.3802, -84.2814),
            lab(50.0, 0.0, -82.7485),
            1.0000,
        ),
        (lab(50.0, 2.5, 0.0), lab(50.0, 0.0, -2.5), 4.3065),
        (lab(50.0, 2.5, 0.0), lab(73.0, 25.0, -18.0), 27.1492),
        (lab(50.0, 2.5, 0.0), lab(61.0, -5.0, 29.0), 22.8977),
    ];
    for (x, y, expected) in pairs {
        let got = delta_e2000(x, y);
        assert!(
            (got - expected).abs() < 1e-3,
            "{x:?} {y:?}: {got} != {expected}"
        );
        assert!((delta_e2000(y, x) - got).abs() < 1e-9, "symmetric");
    }
}

#[test]
fn lab_of_white_and_black() {
    let white = lab_from_srgb8([255, 255, 255]);
    assert!((white.l - 100.0).abs() < 0.01 && white.a.abs() < 0.01 && white.b.abs() < 0.01);
    let black = lab_from_srgb8([0, 0, 0]);
    assert!(black.l.abs() < 1e-9);
    // sRGB red is about L 53.2, a 80.1, b 67.2.
    let red = lab_from_srgb8([255, 0, 0]);
    assert!(
        (red.l - 53.24).abs() < 0.1 && (red.a - 80.09).abs() < 0.2 && (red.b - 67.20).abs() < 0.2
    );
}

#[test]
fn the_reference_round_trips_the_chart() {
    for patch in chart() {
        let back = reference_bt709_video(encode_bt709_video(patch.rgb));
        for i in 0..3 {
            assert!(
                patch.rgb[i].abs_diff(back[i]) <= 2,
                "{}: {:?} came back as {back:?}",
                patch.name,
                patch.rgb
            );
        }
    }
}

#[test]
fn video_range_levels_are_the_standard_ones() {
    assert_eq!(encode_bt709_video([0, 0, 0]), [16, 128, 128]);
    assert_eq!(encode_bt709_video([255, 255, 255]), [235, 128, 128]);
    assert_eq!(expand_to_full_range([16, 128, 128]), [0, 128, 128]);
    assert_eq!(expand_to_full_range([235, 128, 128]), [255, 128, 128]);
}

#[test]
fn shipped_gpui_is_visibly_off_on_saturated_colours() {
    let rows = measure(drawn_by_shipped_gpui);
    assert!(
        !passes(&rows),
        "the BT.601 shader must fail the agreed limits"
    );
    let red = rows.iter().find(|r| r.name == "100% red").unwrap();
    assert!(red.de2000 > 4.0 && red.max_channel > 10, "{red:?}");
    let ramp = rows.iter().find(|r| r.name == "ramp 128").unwrap();
    assert!(
        ramp.max_channel <= 1,
        "greys are not affected by the matrix: {ramp:?}"
    );
}

#[test]
fn a_perfect_pipeline_passes() {
    let rows = measure(reference_bt709_video);
    assert!(passes(&rows));
    assert!(rows.iter().all(|r| r.de2000 == 0.0 && r.max_channel == 0));
}

#[test]
fn the_table_lists_every_patch() {
    let rows = measure(drawn_by_shipped_gpui);
    let table = markdown(&rows);
    assert_eq!(
        table.lines().filter(|l| l.starts_with("| ")).count(),
        rows.len() + 1
    );
    assert!(table.contains("Worst ΔE2000"));
}

#[test]
fn patched_gpui_passes_the_agreed_limits_for_both_buffer_formats() {
    for (name, drawn) in [
        ("420f", drawn_by_patched_gpui_420f as fn([u8; 3]) -> [u8; 3]),
        ("420v", drawn_by_patched_gpui_420v),
    ] {
        let rows = measure(drawn);
        let bad: Vec<_> = rows
            .iter()
            .filter(|r| r.de2000 > MAX_DELTA_E2000 || r.max_channel > MAX_CHANNEL_ERROR)
            .collect();
        assert!(bad.is_empty(), "{name}: {bad:?}");
    }
}

/// Reads a recorded Metal readback: one `r g b` line per chart patch.
fn recorded(text: &str) -> Vec<[u8; 3]> {
    text.lines()
        .map(|l| {
            let v: Vec<u8> = l.split_whitespace().map(|n| n.parse().unwrap()).collect();
            [v[0], v[1], v[2]]
        })
        .collect()
}

// The four files in `fixtures/` were read back from a real Metal render of the chart as a
// `CVPixelBuffer` through `surface()` (see docs/vendor-gpui.md, "How the GPU check was made"),
// on an Apple M2. The shipped GPUI must reproduce the model's numbers exactly, which shows the
// model is right; the patched one must pass.

#[test]
fn recorded_gpu_output_of_shipped_gpui_matches_the_model_and_fails() {
    let rows = measure_captured(&recorded(include_str!("../fixtures/gpu-shipped-420f.txt")));
    let model = measure(drawn_by_shipped_gpui);
    for (gpu, model) in rows.iter().zip(&model) {
        assert_eq!(gpu.drawn, model.drawn, "{}", gpu.name);
    }
    assert!(!passes(&rows));
}

#[test]
fn recorded_gpu_output_of_patched_gpui_passes() {
    for (name, text) in [
        ("420f", include_str!("../fixtures/gpu-patched-420f.txt")),
        ("420v", include_str!("../fixtures/gpu-patched-420v.txt")),
    ] {
        let rows = measure_captured(&recorded(text));
        assert!(passes(&rows), "{name}: {}", markdown(&rows));
    }
}

#[test]
fn recorded_gpu_output_without_a_tag_keeps_the_upstream_look() {
    let rows = measure_captured(&recorded(include_str!(
        "../fixtures/gpu-patched-420f-untagged.txt"
    )));
    assert!(
        !passes(&rows),
        "an untagged buffer is still decoded as BT.601"
    );
}

#[test]
fn bt601_full_range_matches_the_shader_it_replaces() {
    let ours = ycbcr::transform(ycbcr::Matrix::Bt601, true);
    for col in 0..4 {
        for row in 0..4 {
            let (a, b) = (ours[col][row], GPUI_SHIPPED[col][row]);
            // The offsets differ by 0.0027 on purpose: 8-bit chroma is centred on 128/255.
            assert!((a - b).abs() < 0.004, "[{col}][{row}] {a} vs {b}");
        }
    }
}

#[test]
fn matrix_names_follow_corevideo() {
    use ycbcr::Matrix;
    assert_eq!(Matrix::from_cv_name("ITU_R_601_4"), Some(Matrix::Bt601));
    assert_eq!(Matrix::from_cv_name("ITU_R_709_2"), Some(Matrix::Bt709));
    assert_eq!(Matrix::from_cv_name("ITU_R_2020"), Some(Matrix::Bt2020));
    assert_eq!(Matrix::from_cv_name("SMPTE_240M_1995"), None);
    assert_eq!(ycbcr::DEFAULT_MATRIX, Matrix::Bt601);
}

#[test]
fn bt2020_primaries_decode_to_their_corners() {
    // Full-range BT.2020: encode red with Kr 0.2627, Kb 0.0593 and decode it again.
    let t = ycbcr::transform(ycbcr::Matrix::Bt2020, true);
    let (kr, kb) = (0.2627_f64, 0.0593_f64);
    let y = kr;
    let cb = (0.0 - y) / (2.0 * (1.0 - kb));
    let cr = (1.0 - y) / (2.0 * (1.0 - kr));
    let to8 = |v: f64| (v * 255.0).round().clamp(0.0, 255.0) as u8;
    let out = apply(
        &t,
        [to8(y), to8(cb + 128.0 / 255.0), to8(cr + 128.0 / 255.0)],
    );
    assert!(out[0] >= 253 && out[1] <= 2 && out[2] <= 2, "{out:?}");
}
