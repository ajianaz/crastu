# Spike: decoded video frames in GPUI on macOS

Issue #3. Run on 2026-11-11. Code: [`spikes/gpui-video`](../../spikes/gpui-video) (standalone crate, not part of the workspace).

## Question

The editor and the auto-clipper need a smooth preview. GPUI has two ways to put pixels on screen:

- `img()` with a CPU `RenderImage` (BGRA), on every platform. Each frame is uploaded to the sprite atlas.
- `surface()` with a `CVPixelBuffer`, macOS only. The Metal renderer samples it directly.

Which one should the v1 preview use, and what will Windows need?

## Result in short

- **Use VideoToolbox hardware decode straight into `surface()` on macOS.** At 1080p30 it shows 29.8 frames per second with 2.9 % of one core and 81 MB; at 4K30, 29.9 frames per second with 2.8 % and 105 MB. The software paths cost 10 to 40 times more CPU and 2 to 5 times more memory.
- **`surface()` only accepts NV12 full range (`420f`)**, and its shader uses BT.601 coefficients. H.264 and HEVC from cameras and phones are usually BT.709 video range. The spike gets `420f` from VideoToolbox (see below), but colours are still converted with BT.601, so they are slightly off. Fine for a preview; the editor needs a fix (follow-up 1, measured and patched in the section on the colour matrix).
- **`img()` is the portable fallback** and the Windows path. It is fine at 1080p (one third to one half of a core) and heavy at 4K (114 % of a core, 522 MB). Decode at preview size, not source size, when this path is used.
- **Neither `surface()` path reached 60 frames per second** with a 1080p60 clip (34.5 and 48.5), while `img()` did (58.6). Not explained yet (follow-up 2).

## Setup

| Item | Value |
|---|---|
| Machine | Mac mini (Mac14,3), Apple M2 (8 CPU cores, 10 GPU cores), 8 GB, macOS 26.5.1 |
| Display | External 1366 × 768 at 60 Hz, not Retina; window 1280 × 720 |
| GPUI | `gpui-pre` 0.3.12 and `gpui-pre-platform` 0.3.12 (Zed rev `d2a4fb6`), `runtime_shaders` feature |
| GPUI Kit | `gpui-component` 0.9.1 pins `gpui-pre =0.3.12`, so the pair in issue #3 is consistent |
| FFmpeg | 9.0.2 (Homebrew, GPL build, used for measuring only) through `ffmpeg-next` 9.0.0 |
| Clips | H.264 High, BT.709 video range, 20 s, `testsrc2` with temporal noise: 1080p30 at 12 Mb/s, 1080p60 at 20 Mb/s, 2160p30 at 45 Mb/s |

## Method

One binary, three paths:

| Path | Decode | Hand-off to GPUI |
|---|---|---|
| `img` | FFmpeg software, frame threads | swscale to BGRA, copy into a `RenderImage`, `img()`; the previous frame is dropped from the atlas with `window.drop_image` |
| `surface-copy` | FFmpeg software, frame threads | swscale to NV12 full range, copy into a pooled IOSurface-backed `CVPixelBuffer`, `surface()` |
| `surface-vt` | FFmpeg with the VideoToolbox hwaccel | the decoder's `CVPixelBuffer` is retained and passed to `surface()`, no copy |

A decoder thread paces frames at the clip's frame rate and loops the file. The UI keeps only the newest frame; frames replaced before `render()` ran count as skipped. After a 2 s warm-up the program measures for 20 s: frames shown, the interval between shown frames, process CPU time (user + system) divided by wall time, and peak resident memory. One run per cell.

## Results

| Clip | Path | Frames shown/s | Skipped | CPU (% of one core) | Peak RSS (MB) | Frame interval p50 / p95 / max (ms) |
|---|---|---|---|---|---|---|
| 1080p30 | `img` | 26.1 | 82 | 32.3 | 200 | 33.5 / 33.5 / 50.3 |
| 1080p30 | `surface-copy` | 30.0 | 1 | 28.3 | 153 | 33.5 / 33.5 / 50.2 |
| 1080p30 | `surface-vt` | 29.8 | 3 | 2.9 | 81 | 33.5 / 33.5 / 50.2 |
| 1080p60 | `img` | 58.6 | 28 | 51.7 | 199 | 16.7 / 16.8 / 33.5 |
| 1080p60 | `surface-copy` | 34.5 | 513 | 37.8 | 152 | 33.5 / 33.5 / 33.6 |
| 1080p60 | `surface-vt` | 48.5 | 231 | 4.1 | 82 | 16.7 / 33.5 / 42.2 |
| 2160p30 | `img` | 29.7 | 6 | 114.1 | 522 | 33.5 / 33.5 / 83.7 |
| 2160p30 | `surface-copy` | 29.6 | 8 | 89.5 | 394 | 33.5 / 33.5 / 117.1 |
| 2160p30 | `surface-vt` | 29.9 | 2 | 2.8 | 105 | 33.5 / 33.5 / 103.4 |

Conversion cost on the decoder thread (p50 / p95 / p99, ms): BGRA 0.8 / 1.3 / 2.9 at 1080p and 3.3 / 7.8 / 16.2 at 4K; NV12 copy 0.2 / 1.1 / 2.8 at 1080p and 0.9 / 5.2 / 20.3 at 4K. VideoToolbox decode latency averaged 3.0 ms at 1080p and 7.9 ms at 4K.

## Findings

1. **Zero copy works, with one trick.** GPUI's Metal renderer rejects any `CVPixelBuffer` that is not `420f` ("unsupported surface pixel format"). FFmpeg asks VideoToolbox for `420v` when the stream is video range. Marking the codec context as full range inside the `get_format` callback makes FFmpeg request `420f`, and VideoToolbox expands the range while decoding. Every frame in the `surface-vt` runs arrived as `420f`.
2. **Colour matrix.** The surface shader hard-codes BT.601 full-range coefficients. BT.709 content (nearly all HD and 4K video) is therefore shown with a small hue and saturation error. Acceptable for picking clips; not for an editor that promises what you see is what you export.
3. **`img()` needs explicit atlas cleanup.** Every `RenderImage` stays in the sprite atlas until `window.drop_image` is called. The spike drops the previous frame on each swap; without that, memory grows with every frame.
4. **Occluded windows stop rendering.** GPUI stops its display link when the window is hidden, minimised or covered (`windowDidChangeOcclusionState`). The first measurements showed zero frames for that reason; the spike uses a pop-up level window. For Crastu: the playback clock must live with the decoder or audio output, never in `render()`, and a hidden preview should pause decoding to save power.
5. **Metal toolchain.** Xcode 26 no longer ships the `metal` compiler; GPUI's build script fails without it. Either install it (`xcodebuild -downloadComponent MetalToolchain`, also on the macOS CI runner) or enable `runtime_shaders`, which compiles the shaders at launch. Decision 32 keeps `runtime_shaders` for CI and release builds until the start-up cost is measured.
6. **60 fps.** Both `surface()` paths stayed near 30 or 48 frames per second on the 60 fps clip while decode kept up (VideoToolbox latency 2.5 ms). `img()` on the same clip reached 58.6. The cause is not known yet; it may be in how GPUI schedules frames that contain surfaces.
7. **Unsafe code.** The VideoToolbox path needs raw FFmpeg and CoreVideo calls (hardware device, `get_format` callback, `CVPixelBuffer` from `data[3]`). In Crastu this belongs in an FFI crate that opts in to `unsafe` and documents why (AGENTS.md).

## Colour matrix measurement (issue #39)

Follow-up 1 above, measured. `cargo xtask colour-report` (crate `xtask/colour`, no GPU, no dependencies) encodes a chart as 8-bit NV12 video-range BT.709, the way camera and phone H.264 arrives, and decodes it two ways: the exact BT.709 equations (the reference) and what GPUI draws. The chart has the 75% colour bars, the 100% primaries and secondaries, a nine-step grey ramp and two memory colours (skin, sky). Colours are compared in CIELAB (D65) with CIEDE2000 (the implementation is checked against the published test pairs of Sharma, Wu and Dalal) and by the largest single-channel error in 8-bit steps.

GPUI's side is the shader in `gpui-pre-apple` 0.3.12, `src/shaders.metal`, `surface_fragment` at line 884, matrix at lines 890 to 894: the matrix columns (1, 1, 1), (0, -0.3441, 1.7720), (1.4020, -0.7141, 0) and offset (-0.7010, 0.5291, -0.8860) are BT.601 on full-range input. The VideoToolbox path of finding 1 hands over BT.709 values stretched to full range, so that is what the model feeds it.

Agreed limit, per patch: **CIEDE2000 at most 1.0 and no channel more than 2 steps off.** One CIEDE2000 unit is below the just noticeable difference (about 2.3 in CIELAB); 1.0 leaves room for 8-bit rounding, which alone costs up to 0.4 in the saturated patches. The issue's "within an agreed colour difference" means this.

Result for GPUI as shipped (reference decode in the "Reference" column):

| Patch | sRGB in | Y Cb Cr (video) | Reference | Drawn | ΔE76 | ΔE2000 | Max channel |
|---|---|---|---|---|---|---|---|
| 75% white | 191 191 191 | 180 128 128 | 191 191 191 | 192 190 192 | 1.3 | 1.70 | 1 |
| 75% yellow | 191 191 0 | 168 44 136 | 191 191 0 | 190 203 8 | 7.2 | 3.77 | 12 |
| 75% cyan | 0 191 191 | 145 147 44 | 0 191 190 | 16 210 190 | 13.1 | 7.77 | 19 |
| 75% green | 0 191 0 | 133 63 52 | 0 191 0 | 15 223 6 | 15.1 | 8.00 | 32 |
| 75% magenta | 191 0 191 | 63 193 204 | 191 0 192 | 178 0 187 | 4.1 | 2.68 | 13 |
| 75% red | 191 0 0 | 51 109 212 | 191 0 1 | 176 0 3 | 6.5 | 3.07 | 15 |
| 75% blue | 0 0 191 | 28 212 120 | 0 0 191 | 2 0 185 | 2.7 | 0.76 | 6 |
| 100% red | 255 0 0 | 63 102 240 | 255 1 0 | 234 0 3 | 8.3 | 4.55 | 21 |
| 100% green | 0 255 0 | 173 42 26 | 0 255 1 | 21 255 10 | 1.0 | 0.17 | 21 |
| 100% blue | 0 0 255 | 32 240 118 | 1 0 255 | 4 0 245 | 4.3 | 1.25 | 10 |
| 100% yellow | 255 255 0 | 219 16 138 | 254 255 0 | 252 255 12 | 1.2 | 0.48 | 12 |
| 100% cyan | 0 255 255 | 188 154 16 | 0 254 255 | 23 255 254 | 1.1 | 0.64 | 23 |
| 100% magenta | 255 0 255 | 78 214 230 | 255 0 254 | 235 0 247 | 6.0 | 3.70 | 20 |
| skin | 194 150 130 | 152 115 148 | 194 150 131 | 191 146 132 | 3.0 | 2.46 | 4 |
| sky | 100 149 237 | 140 172 103 | 100 148 237 | 105 146 233 | 1.8 | 1.31 | 5 |
| ramp 0 | 0 0 0 | 16 128 128 | 0 0 0 | 1 0 1 | 0.5 | 0.66 | 1 |
| ramp 32 | 32 32 32 | 43 128 128 | 31 31 31 | 32 30 32 | 1.8 | 2.28 | 1 |
| ramp 64 | 64 64 64 | 71 128 128 | 64 64 64 | 65 63 65 | 1.6 | 2.05 | 1 |
| ramp 96 | 96 96 96 | 98 128 128 | 95 95 95 | 96 94 96 | 1.5 | 1.93 | 1 |
| ramp 128 | 128 128 128 | 126 128 128 | 128 128 128 | 129 127 129 | 1.4 | 1.83 | 1 |
| ramp 160 | 160 160 160 | 153 128 128 | 160 160 160 | 161 159 161 | 1.4 | 1.76 | 1 |
| ramp 192 | 192 192 192 | 181 128 128 | 192 192 192 | 193 191 193 | 1.3 | 1.70 | 1 |
| ramp 224 | 224 224 224 | 208 128 128 | 224 224 224 | 225 223 225 | 1.3 | 1.66 | 1 |
| ramp 255 | 255 255 255 | 235 128 128 | 255 255 255 | 255 254 255 | 0.7 | 0.83 | 1 |


The same numbers came out of a real Metal render: the chart was written into an IOSurface-backed `CVPixelBuffer` (one 16 by 16 block per patch), drawn through `surface()` into an offscreen texture on an Apple M2 and read back at the block centres. The unpatched crate reproduced the table above exactly (worst 8.00, mean 2.38, 18 of 24 patches over the limit), so the model is right. The error is a hue and saturation shift on every colour (75% green comes out as 15 223 6 instead of 0 191 0, 100% red as 234 0 3 instead of 255 0 0) plus a one-step cast on greys, because the shader centres chroma on 0.5 instead of 128/255.

With the patch of [`docs/vendor-gpui.md`](../vendor-gpui.md) (matrix and range per surface, read from the buffer's `kCVImageBufferYCbCrMatrixKey` tag and its pixel format), the same render gives:

| Surface | Worst CIEDE2000 | Mean | Patches over the limit | Largest channel error |
|---|---|---|---|---|
| `420f` tagged BT.709 (the VideoToolbox output of decision 27) | 0.41 | 0.07 | 0 | 1 |
| `420v` tagged BT.709 (no range expansion) | 0.00 | 0.00 | 0 | 0 |
| `420f`, no tag (upstream behaviour kept) | 8.39 | 1.74 | 10 | 33 |

The committed check is `cargo test -p xtask-colour`: it runs the shipped matrix builder (the very file under `vendor/`) over the chart and holds it to the limit, and it holds the four recorded Metal read-backs (`xtask/colour/fixtures/`) to the same limit (shipped fails, patched passes). The read-backs cannot be regenerated in CI; how they were made is in `docs/vendor-gpui.md`.

Not covered: a visual comparison on a display (the offscreen read-back is numeric), BT.2020 with real HDR transfer curves (the matrix is there and unit-tested, but PQ/HLG need tone mapping this patch does not do), and 10-bit formats (`surface()` still takes 8-bit 4:2:0 only).

## Recommendation for the v1 preview

1. macOS: VideoToolbox decode into `surface()`, forcing `420f` as above. This covers H.264, HEVC and ProRes on Apple Silicon.
2. macOS fallback when VideoToolbox cannot decode a stream: software decode into a pooled `CVPixelBuffer` (`surface-copy`), scaled to the preview size. It uses less CPU and memory than `img()`.
3. A `FrameSink` seam in `crastu-media` (or the preview crate) so the decoder hands over either a platform buffer or a CPU image, and the UI picks `surface()` or `img()`. The clock stays outside the UI.
4. Before P4 (editor): fix the colour matrix (follow-up 1) and the 60 fps behaviour (follow-up 2). P3 (auto-clipper) can ship with the current preview.

## What Windows will need (P5)

GPUI has no `surface()` on Windows, so the preview uses `img()`: decode (D3D11VA through FFmpeg, or software), convert to BGRA at preview size, upload. The numbers above suggest 1080p is comfortable and 4K needs downscaling before conversion. A texture path (D3D11 shared handles into GPUI) is a later research item, as the roadmap's risk table already says.

## Follow-ups

1. Colour: done as a patch on the pinned `gpui-pre-apple` (`docs/vendor-gpui.md`, measurement above); the upstream proposal is written there and still to be filed. The media layer must tag its buffers with their matrix.
2. 60 fps with `surface()`: profile GPUI frame scheduling with a 60 fps clip on a Retina display.
3. Repeat on a Retina display and with real camera footage (the clips here are synthetic) before P3.
4. Measure a shipping configuration: LGPL shared FFmpeg build (VideoToolbox works without GPL parts) and precompiled shaders.

## Reproduce

```sh
cd spikes/gpui-video
cargo build --release
# test clip (any H.264 file works)
ffmpeg -f lavfi -i "testsrc2=size=1920x1080:rate=30,noise=alls=8:allf=t" -t 20 \
  -c:v libx264 -preset fast -b:v 12M -maxrate 12M -bufsize 12M -pix_fmt yuv420p \
  -colorspace bt709 -color_primaries bt709 -color_trc bt709 -color_range tv 1080p30.mp4
./target/release/spike-gpui-video surface-vt 1080p30.mp4 20   # or img, surface-copy
```

The window opens above other windows so that GPUI keeps drawing; set `SPIKE_NORMAL_WINDOW=1` for a normal window and `SPIKE_DEBUG=1` for per-second counters. Each run prints one JSON line.
