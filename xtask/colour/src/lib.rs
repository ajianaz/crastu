//! A deterministic, GPU-free check of the YCbCr to RGB conversion behind GPUI's `surface()`
//! (issue #39, spike `docs/spikes/gpui-video.md`).
//!
//! A colour chart is encoded as 8-bit NV12 video-range BT.709, the way H.264 and HEVC from
//! cameras and phones arrive. It is then decoded two ways: by the exact BT.709 equations
//! (the reference) and by what GPUI draws. Each patch is compared in CIELAB with CIEDE2000.

pub mod de;
/// The matrix builder of the patched `gpui-pre-apple` (`vendor/gpui-pre-apple`), included as
/// source so that the test covers the code that ships. Its Metal side needs a GPU.
#[allow(dead_code)]
#[path = "../../../vendor/gpui-pre-apple/src/ycbcr.rs"]
pub mod ycbcr;

use de::{delta_e76, delta_e2000, lab_from_srgb8};

/// Agreed limit for the colour error of one patch: CIEDE2000 at most 1.0 (below the just
/// noticeable difference of about 2.3 in CIELAB, with room for 8-bit rounding in dark greys).
pub const MAX_DELTA_E2000: f64 = 1.0;
/// Agreed limit for the largest single-channel error of a patch, in 8-bit steps.
pub const MAX_CHANNEL_ERROR: u8 = 2;

/// A conversion as the 4x4 matrix GPUI hands to the Metal shader: column-major, applied to
/// `[y, cb, cr, 1]` with every component normalised to 0..1 as the texture sampler returns it.
pub type Transform = [[f32; 4]; 4];

/// The matrix of `surface_fragment` in `gpui-pre-apple` 0.3.12, `src/shaders.metal` (function at
/// line 884, matrix at lines 890 to 894; Zed rev d2a4fb6): BT.601 coefficients on full-range input. The fourth column
/// folds in the 0.5 chroma offset (0.701 = 1.402 / 2 and so on).
pub const GPUI_SHIPPED: Transform = [
    [1.0000, 1.0000, 1.0000, 0.0000],
    [0.0000, -0.3441, 1.7720, 0.0000],
    [1.4020, -0.7141, 0.0000, 0.0000],
    [-0.7010, 0.5291, -0.8860, 1.0000],
];

/// Applies a [`Transform`] to 8-bit NV12 samples and returns 8-bit RGB (rounded, clamped), like
/// the shader followed by an 8-bit render target.
pub fn apply(transform: &Transform, ycc: [u8; 3]) -> [u8; 3] {
    let v = [
        f32::from(ycc[0]) / 255.0,
        f32::from(ycc[1]) / 255.0,
        f32::from(ycc[2]) / 255.0,
        1.0,
    ];
    let channel = |row: usize| {
        let sum: f32 = (0..4).map(|col| transform[col][row] * v[col]).sum();
        (sum * 255.0).round().clamp(0.0, 255.0) as u8
    };
    [channel(0), channel(1), channel(2)]
}

/// The BT.709 luma weights.
const KR: f64 = 0.2126;
const KB: f64 = 0.0722;

fn q(v: f64) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}

/// Encodes 8-bit RGB as video-range BT.709 (Y 16..235, Cb and Cr 16..240), 8 bits.
pub fn encode_bt709_video(rgb: [u8; 3]) -> [u8; 3] {
    let [r, g, b] = rgb.map(|v| f64::from(v) / 255.0);
    let y = KR * r + (1.0 - KR - KB) * g + KB * b;
    let cb = (b - y) / (2.0 * (1.0 - KB));
    let cr = (r - y) / (2.0 * (1.0 - KR));
    [
        q(16.0 + 219.0 * y),
        q(128.0 + 224.0 * cb),
        q(128.0 + 224.0 * cr),
    ]
}

/// The reference render: exact BT.709 video-range equations, 8-bit RGB out.
pub fn reference_bt709_video(ycc: [u8; 3]) -> [u8; 3] {
    let y = (f64::from(ycc[0]) - 16.0) / 219.0;
    let cb = (f64::from(ycc[1]) - 128.0) / 224.0;
    let cr = (f64::from(ycc[2]) - 128.0) / 224.0;
    let r = y + 2.0 * (1.0 - KR) * cr;
    let b = y + 2.0 * (1.0 - KB) * cb;
    let g = (y - KR * r - KB * b) / (1.0 - KR - KB);
    [q(r * 255.0), q(g * 255.0), q(b * 255.0)]
}

/// What the VideoToolbox decoder hands to `surface()` after the spike's `420f` trick (finding 1):
/// the same BT.709 values, stretched from video range to full range, still 8 bits.
pub fn expand_to_full_range(ycc: [u8; 3]) -> [u8; 3] {
    [
        q((f64::from(ycc[0]) - 16.0) * 255.0 / 219.0),
        q((f64::from(ycc[1]) - 128.0) * 255.0 / 224.0 + 128.0),
        q((f64::from(ycc[2]) - 128.0) * 255.0 / 224.0 + 128.0),
    ]
}

/// A named patch of the chart, as 8-bit sRGB.
#[derive(Clone, Copy, Debug)]
pub struct Patch {
    pub name: &'static str,
    pub rgb: [u8; 3],
}

const fn p(name: &'static str, r: u8, g: u8, b: u8) -> Patch {
    Patch {
        name,
        rgb: [r, g, b],
    }
}

/// The chart: 75% colour bars, 100% primaries and secondaries, a grey ramp and two
/// memory colours (skin, sky).
pub fn chart() -> Vec<Patch> {
    let mut patches = vec![
        p("75% white", 191, 191, 191),
        p("75% yellow", 191, 191, 0),
        p("75% cyan", 0, 191, 191),
        p("75% green", 0, 191, 0),
        p("75% magenta", 191, 0, 191),
        p("75% red", 191, 0, 0),
        p("75% blue", 0, 0, 191),
        p("100% red", 255, 0, 0),
        p("100% green", 0, 255, 0),
        p("100% blue", 0, 0, 255),
        p("100% yellow", 255, 255, 0),
        p("100% cyan", 0, 255, 255),
        p("100% magenta", 255, 0, 255),
        p("skin", 194, 150, 130),
        p("sky", 100, 149, 237),
    ];
    for (name, level) in [
        ("ramp 0", 0),
        ("ramp 32", 32),
        ("ramp 64", 64),
        ("ramp 96", 96),
        ("ramp 128", 128),
        ("ramp 160", 160),
        ("ramp 192", 192),
        ("ramp 224", 224),
        ("ramp 255", 255),
    ] {
        patches.push(p(name, level, level, level));
    }
    patches
}

/// The measurement of one patch.
#[derive(Clone, Debug)]
pub struct Row {
    pub name: &'static str,
    pub source: [u8; 3],
    pub video_ycc: [u8; 3],
    pub reference: [u8; 3],
    pub drawn: [u8; 3],
    pub de76: f64,
    pub de2000: f64,
    pub max_channel: u8,
}

/// Compares the reference render with a pipeline: `drawn` maps the video-range BT.709 samples
/// of a patch to the RGB that GPUI puts on screen.
pub fn measure(drawn: impl Fn([u8; 3]) -> [u8; 3]) -> Vec<Row> {
    chart()
        .into_iter()
        .map(|patch| {
            let video_ycc = encode_bt709_video(patch.rgb);
            let reference = reference_bt709_video(video_ycc);
            let drawn = drawn(video_ycc);
            let (lr, ld) = (lab_from_srgb8(reference), lab_from_srgb8(drawn));
            Row {
                name: patch.name,
                source: patch.rgb,
                video_ycc,
                reference,
                drawn,
                de76: delta_e76(lr, ld),
                de2000: delta_e2000(lr, ld),
                max_channel: (0..3)
                    .map(|i| reference[i].abs_diff(drawn[i]))
                    .max()
                    .unwrap_or(0),
            }
        })
        .collect()
}

/// The chart as video-range BT.709 NV12 samples, in chart order: what a test or a hands-on
/// check writes into a `CVPixelBuffer`, one patch per block.
pub fn chart_video_ycc() -> Vec<[u8; 3]> {
    chart().iter().map(|p| encode_bt709_video(p.rgb)).collect()
}

/// Compares RGB read back from a real render (one value per patch, in chart order) with the
/// reference. Panics if the counts differ.
pub fn measure_captured(captured: &[[u8; 3]]) -> Vec<Row> {
    assert_eq!(
        captured.len(),
        chart().len(),
        "one captured colour per chart patch"
    );
    let next = std::cell::Cell::new(0);
    measure(|_| {
        let i = next.get();
        next.set(i + 1);
        captured[i]
    })
}

/// GPUI as it ships: range expanded by VideoToolbox, then [`GPUI_SHIPPED`].
pub fn drawn_by_shipped_gpui(video_ycc: [u8; 3]) -> [u8; 3] {
    apply(&GPUI_SHIPPED, expand_to_full_range(video_ycc))
}

/// The patched GPUI with the decoder's `420f` buffers (VideoToolbox, finding 1 of the spike),
/// tagged BT.709: range expanded by the decoder, then the BT.709 full-range matrix.
pub fn drawn_by_patched_gpui_420f(video_ycc: [u8; 3]) -> [u8; 3] {
    apply(
        &ycbcr::transform(ycbcr::Matrix::Bt709, true),
        expand_to_full_range(video_ycc),
    )
}

/// The patched GPUI with a `420v` buffer tagged BT.709: the shader undoes the range itself.
pub fn drawn_by_patched_gpui_420v(video_ycc: [u8; 3]) -> [u8; 3] {
    apply(&ycbcr::transform(ycbcr::Matrix::Bt709, false), video_ycc)
}

/// True when every row is inside the agreed limits.
pub fn passes(rows: &[Row]) -> bool {
    rows.iter()
        .all(|r| r.de2000 <= MAX_DELTA_E2000 && r.max_channel <= MAX_CHANNEL_ERROR)
}

/// A Markdown table of the measurement.
pub fn markdown(rows: &[Row]) -> String {
    let mut out = String::from(
        "| Patch | sRGB in | Y Cb Cr (video) | Reference | Drawn | ΔE76 | ΔE2000 | Max channel |\n|---|---|---|---|---|---|---|---|\n",
    );
    let triple = |t: [u8; 3]| format!("{} {} {}", t[0], t[1], t[2]);
    for r in rows {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {:.1} | {:.2} | {} |\n",
            r.name,
            triple(r.source),
            triple(r.video_ycc),
            triple(r.reference),
            triple(r.drawn),
            r.de76,
            r.de2000,
            r.max_channel
        ));
    }
    let worst = rows.iter().map(|r| r.de2000).fold(0.0, f64::max);
    let mean = rows.iter().map(|r| r.de2000).sum::<f64>() / rows.len().max(1) as f64;
    out.push_str(&format!(
        "\nWorst ΔE2000 {worst:.2}, mean {mean:.2}, patches over {MAX_DELTA_E2000}: {}.\n",
        rows.iter().filter(|r| r.de2000 > MAX_DELTA_E2000).count()
    ));
    out
}

#[cfg(test)]
mod tests;
