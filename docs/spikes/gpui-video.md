# Spike: decoded video frames in GPUI on macOS

Issue #3. Run on 2026-11-11. Code: [`spikes/gpui-video`](../../spikes/gpui-video) (standalone crate, not part of the workspace).

## Question

The editor and the auto-clipper need a smooth preview. GPUI has two ways to put pixels on screen:

- `img()` with a CPU `RenderImage` (BGRA), on every platform. Each frame is uploaded to the sprite atlas.
- `surface()` with a `CVPixelBuffer`, macOS only. The Metal renderer samples it directly.

Which one should the v1 preview use, and what will Windows need?

## Result in short

- **Use VideoToolbox hardware decode straight into `surface()` on macOS.** At 1080p30 it shows 29.8 frames per second with 2.9 % of one core and 81 MB; at 4K30, 29.9 frames per second with 2.8 % and 105 MB. The software paths cost 10 to 40 times more CPU and 2 to 5 times more memory.
- **`surface()` only accepts NV12 full range (`420f`)**, and its shader uses BT.601 coefficients. H.264 and HEVC from cameras and phones are usually BT.709 video range. The spike gets `420f` from VideoToolbox (see below), but colours are still converted with BT.601, so they are slightly off. Fine for a preview; the editor needs a fix (follow-up 1).
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

## Recommendation for the v1 preview

1. macOS: VideoToolbox decode into `surface()`, forcing `420f` as above. This covers H.264, HEVC and ProRes on Apple Silicon.
2. macOS fallback when VideoToolbox cannot decode a stream: software decode into a pooled `CVPixelBuffer` (`surface-copy`), scaled to the preview size. It uses less CPU and memory than `img()`.
3. A `FrameSink` seam in `crastu-media` (or the preview crate) so the decoder hands over either a platform buffer or a CPU image, and the UI picks `surface()` or `img()`. The clock stays outside the UI.
4. Before P4 (editor): fix the colour matrix (follow-up 1) and the 60 fps behaviour (follow-up 2). P3 (auto-clipper) can ship with the current preview.

## What Windows will need (P5)

GPUI has no `surface()` on Windows, so the preview uses `img()`: decode (D3D11VA through FFmpeg, or software), convert to BGRA at preview size, upload. The numbers above suggest 1080p is comfortable and 4K needs downscaling before conversion. A texture path (D3D11 shared handles into GPUI) is a later research item, as the roadmap's risk table already says.

## Follow-ups

1. Colour: propose a GPUI change that takes the YCbCr matrix and range per surface (BT.601/BT.709, video/full), or carry a small patch on the pinned `gpui-pre` until upstream has it.
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
