# Spike: Vello and Parley headless, determinism and speed for `crastu-render` slice 2

Issue #35. Run on 2026-11-13. Code: [`spikes/render-gpu`](../../spikes/render-gpu) (standalone crate with its own `[workspace]`, throwaway; no fonts, images or outputs are committed). Versions, licences and MSRV of the stack were collected earlier in [`render-gpu-stack.md`](render-gpu-stack.md) and re-read from crates.io on 2026-11-13 (`vello` 0.12.0, `vello_cpu` 0.4.0, `parley` 0.13.0, `fontique` 0.13.0, `wgpu` 30.0.2, `peniko` 0.6.1, `kurbo` 0.13.1; `Cargo.lock` also has `skrifa` 0.49.0 and `harfrust` 0.15.0).

## Question

Can `crastu-render` draw its frames with Vello (2D) and Parley (text) with no window, produce the same bytes for the same time whatever the order, and render a 60 s 1080x1920 clip with animated captions faster than real time? Can the existing `Painter` and `TextLayout` traits sit on top of them, and does the dependency tree pass `cargo deny` and compile for iOS and Android?

## Result in short

- **It works headless on Metal.** An offscreen 1080x1920 frame (14 rounded rects, a lower-third card, a pill behind the captions, Inter SemiBold text laid out by Parley, per-word colour, scale and a karaoke clip) renders and reads back through wgpu with no window. Parley used only the font file registered from the repo (`crates/ui/crastu-ui/assets/fonts/inter/Inter-SemiBold.ttf`, the default pairing of `tokens.json`), with `parley`'s default `system` feature switched off: the frame is byte-identical to the build with system fonts (SHA-256 prefix `333d9e280a8ec926` on frame 20 in both).
- **Speed is not the problem.** 1800 frames (60 s at 30 fps) took 4.6 to 9.3 s on an M2 with a full CPU readback of every frame: 6.4x to 13.0x real time, on a machine that was busy with other builds the whole time. M1 was not available, so the "faster than real time on an M1" bar is met on an M2 with a wide margin and is not measured on an M1.
- **The GPU path is not byte-deterministic once glyphs are scaled.** Same frame, same device, same process: two different results, one pixel by one level, flipping between the two. 1 or 2 of 120 frames differed in every run; 2 or 3 of 1800 in a forward against reverse pass. With the pop-in scale switched off, 0 of 120 frames differed in any order, on fresh devices, and across two processes. The test of decision 39 ("byte-identical on one machine") therefore holds for the CPU renderer and needs a one-level tolerance on the GPU renderer.
- **`vello_cpu` was order-independent and fast enough for this composition**: 0 of 300 (single thread) and 0 of 1800 (4 threads) frames differed between a forward and a reverse pass, frame 20 had the same hash in two processes (0 and 4 threads; the reverse pass reuses the warm Parley page cache and glyph resources, so this shows order independence in one process, one machine and one SIMD level, not cross-machine determinism), and building the scene plus rendering took about 2.6 ms per frame with its single-threaded dispatcher (one noisy 300-frame run covering the first 10 s, see below). Its pixels differ from the GPU's: 18% of the pixels of one frame, mostly by one level, up to 217 at glyph edges. So preview and export must use the same backend if they are to match byte for byte.
- **Recommendation**: Parley for text in both cases; `vello_cpu` as the reference renderer for export, golden tests and (at first) preview; the GPU renderer is added only if a heavier composition (video layers, blur, many layers: all unmeasured here) is too slow on the CPU. Decision 41 proposal below.

## Setup

| Item | Value |
|---|---|
| Machine | Mac mini (Mac14,3), Apple M2, 8 GB, macOS 26.5.1, rustc 1.99.0. Load average was 9 to 17 for the whole session (other builds), so every time below is a pessimistic one and the spread between runs is large |
| GPU | wgpu 30.0.2 on Metal; `AdapterInfo` reported `Apple M2`, `IntegratedGpu`, backend `Metal` |
| Vello GPU | `vello::Renderer` on a `vello::util::RenderContext` with no surface, `AaConfig::Area` only (`AaSupport::area_only()`), target `Rgba8Unorm` storage texture, copied to a mapped buffer for readback |
| Vello CPU | `vello_cpu` 0.4.0, default features plus `multithreading`; `RenderContext` plus `Pixmap` |
| Text | Parley 0.13.0 with `default-features = false, features = ["std"]` (no system font discovery), the font registered with `FontContext::collection.register_fonts`; glyph runs drawn with `Scene::draw_glyphs` (GPU) and `glyph_run` (CPU) |
| Frame | 1080x1920, a pure function of `t`: 14 rounded rects (one moves with `t`), a lower-third card and title, a caption pill, pages of 4 words (0.4 s per word, 110 px Inter SemiBold, wrapped to 940 px). Per word: grey before, an analytic damped-cosine pop (scale 1 + 0.22 e^(-9 s) cos(30 s)) from its start, a white base with a yellow copy under a clip layer sized to the played fraction (karaoke) while active, white after. Time is `frame / 30` |
| Tools | `/usr/bin/time -l` for process memory (maximum resident set and peak memory footprint); `std::time::Instant` inside the program for the splits; SHA-256 of the RGBA bytes for equality; `cargo deny` with the repo's `deny.toml`; `cargo check --target` for the mobile targets |

The spike's commands: `ppm <frame> <file>`, `det [dump]`, `scan`, `flaky <frame> <n>`, `speed [--no-readback] [--relayout]`, `cpu <threads> <frames>`. `NOPOP=1`, `NOCLIP=1` and (for `flaky`) `NEWSCENE=1` switch the pop scale off, the karaoke clip off and use a fresh `Scene` per render, for the isolation runs. Three checks were done ad hoc and are not commands in the crate: the build with and without Parley's `system` feature (compare the `ppm 20` hashes), the CPU against GPU pixel comparison (a throwaway script over the PPM files that `ppm` and `cpu` write), and viewing the frames (PPM converted to PNG).

## 1. A frame, headless

Frame 20 (t = 0.667 s) was written as a PPM and viewed: the cards are rounded, the pill is translucent black, "RENDER" is white (spoken), "EVERY" is yellow up to the played fraction with the white base beyond it, "FRAME FROM" is grey (unspoken), and the title is in the white card. The CPU renderer's frame was viewed as well (caption area) and looks the same to the eye.

`Gpu::new` (instance, device, `Renderer` with shaders) took 46 to 252 ms over the runs; the first frame 23 to 153 ms. The Metal shader cache may be warm from earlier runs; a cold install is not measured.

## 2. Determinism

120 frames spread over 60 s (frame numbers `i * 15`), hashed with SHA-256.

| Case | Result (GPU, pop scale on) |
|---|---|
| A: frame 37 rendered twice, same device | identical (`f98a83c6821a9977` both times, in every run) |
| B: 120 frames forward, then in a fixed pseudo-random order, same device | 1 or 2 of 120 differ (runs: 2, 1, 1, 2) |
| C: brand-new instance, device, renderer and font context, forward | 1 or 2 differ (2, 2, 1, 1) |
| D: another brand-new device, reverse of the shuffled order | 1 or 2 differ (1, 1, 1, 2) |
| Two processes, forward pass only | not equal: combined hashes `2c95d898a9627df7` and `d9d2b944bf5fc914` (first binary), `24f3c2c5e2f43bc5` and `57cd344ab6b35bad` (second) |
| 1800 frames forward then reverse, same device (`scan`, run twice) | 2 differ (1290, 132) and 3 differ (282, 276, 132) |

What differs, measured with `flaky <frame> <n>` (the same frame rendered n times on one device):

- Frames 1140 and 1290: two distinct results, in a ratio near half and half (15/15, 9/11, 6/14 ...), switching from run to run of the loop. The difference is **one byte, one pixel, one level** (max channel delta 1): pixel (433, 1039) for frame 1140 and (379, 1048) for frame 1290, on glyph edges in the caption. Frames 132 and 282 also gave two results (the size of their difference was not printed).
- The encoded scene is not what changes: the SHA-256 over the scene's `path_data`, `draw_data`, positioned glyphs, every transform and the per-run transform, glyph range and size was identical for both results (an earlier hash over `path_data` and `draw_data` alone said the same; the glyph side was added after review). What was not compared is Vello's render-time resolution of glyph outlines (its glyph cache lives in the `Renderer`), so the flip is on the render side, GPU or that CPU step. Not root-caused (no `AaConfig` change, no GPU capture).
- It needs the animated scale: with the pop switched off (`NOPOP=1`), the four affected frames gave one result in 20 renders each, B, C and D gave 0 of 120 differing frames, and two processes gave the same combined hash (`fc070ffcb77015fb`).
- It does not need the karaoke clip (`NOCLIP=1` still flips) and it does not come from reusing the `Scene` (a fresh `Scene` per render still flips).
- A test needs a tolerance of one level on at most a few pixels per frame for the GPU path. Seen: at most one pixel, by one level, in a frame. Not a bound: the size of the difference was printed for two flaky frames only.

Vello CPU, same scene:

- 300 frames forward then reverse, single thread: 0 differ. 1800 frames, 4 threads: 0 differ. Frame 20 had SHA-256 prefix `49a0cecae01406dd` in two processes, with 0 and 4 threads.
- Against the GPU, frame 20: 372,270 of 2,073,600 pixels differ (18%): 360,623 by 1 level, the rest up to 217; 1692 pixels differ by more than 40, all on glyph edges (caption and title text). Why (glyph positioning, hinting or anti-aliasing) was not investigated.

## 3. Speed

1800 frames of the animated composition, scene built each frame (page layouts cached), measured inside the program after one warm-up frame. "Readback" copies the 8.3 MB texture to the CPU and unpacks the padded rows every frame, the cost an encoder feed pays.

| Mode | Runs | Per run | Note |
|---|---|---|---|
| GPU, render + full readback | 6 | 4.63 to 9.33 s (193 to 389 fps, 6.4x to 13.0x real time) | the slow run coincided with load average 12+ |
| GPU, render and wait, no readback | 4 | 2.96 to 4.01 s (448 to 607 fps) | one frame at a time, so this is latency, not throughput |
| GPU, the page's 4 words re-laid out by Parley every frame (the title is shaped every frame in every mode) | 1 | 4.05 s | scene build was 0.07 s of it; 39 us per frame for four layouts is within the noise of the non-relayout runs, so Parley layout cost is small but this run does not quantify it |
| Readback alone (idle texture, 300 copies) | 8 | 0.63 to 2.63 ms per frame, typically 0.8 | |
| Scene build (our code; page layouts cached, title shaped every frame) | 5 | 0.07 to 0.18 s for 1800 frames | |
| CPU `vello_cpu`, `num_threads` 0 (its single-threaded dispatcher) | 1 | 300 frames in 0.77 s = 391 fps, 13x real time | SHA-256 of each frame excluded; includes building the scene (Parley, title shaping) and the pixmap render |
| CPU, `num_threads` 2 and 7 (multithreaded dispatcher, that many workers) | 1 each | 255 and 174 fps | slower than the single-threaded dispatcher on this scene and this loaded machine |

Memory from `/usr/bin/time -l` for the whole process: GPU speed runs 52 to 60 MB maximum resident set and 234 to 237 MB peak footprint; the CPU run (1800 frames, 4 threads) 28 MB resident and 21 MB peak footprint. The first CPU timings of this session (9.4 s for 300 frames) were dominated by hashing every frame with SHA-256 (about 35 ms per 8.3 MB frame while the machine was loaded) and are not a renderer figure.

Against the bar of the issue: 60 s at 1080x1920 renders faster than real time here on both renderers, for this composition (flat shapes and 4 words). Not measured: video or image layers (texture upload, `draw_asset`), blur, many more layers, and an M1.

## 4. Fit with `Painter` and `TextLayout`

The spike draws through a six-method `Canvas` trait (`clear`, `fill_rect`, `fill_rrect`, `glyphs`, `push_clip`, `pop_clip`) with one implementation over `vello::Scene` and one over `vello_cpu::RenderContext`, about 40 lines each. No wgpu or Parley type is in the trait and nothing in it is a GPU object, but it still names types that `vello` re-exports (`vello::Glyph`, peniko `Color` and `FontData`, kurbo `Rect`, `RoundedRect` and `Affine`), so the spike's CPU path still depends on the `vello` crate; the production traits should use neutral types (item 2 below). The device, the renderer and the readback stay in a separate struct that the composition never sees. So both traits can be implemented without leaking GPU types. What has to change:

1. **`TextRun` has no font.** It carries text, rect, size, colour, scale and opacity, but not the family, weight or font blob, so a painter cannot know what to draw. Add a font reference (an id into a table registered once, so `tokens.json` fonts and user fonts are both registered by bytes; Parley's `register_fonts` accepted the bundled file with no system backend).
2. **Shaping must happen once, in the layout, and the painter must receive glyphs.** Today `draw_text_run` gets a string and the painter would have to shape it again. Make `TextLayout` return a shaped, backend-neutral run (`font id`, `size`, `Vec<(glyph id, x, y)>`, advance, ascent and descent) and let `draw_text_run` take it. Words are separate runs in the caption layer already, and the spike confirms Parley gives one glyph run per word when each word has its own brush, so per-word colour and scale need no extra API. Cache the shaped words per `(text, size, font)`.
3. **`TextLayout::measure(&self)` and `lines(&self)`.** Parley needs `&mut FontContext` and `&mut LayoutContext`, so the implementation needs interior mutability (a mutex or a per-thread context) or the trait takes `&mut self`. `caption.rs` calls `measure` for every prefix of a line; with Parley each call is a layout. Cheaper: shape the whole page once and read word x positions from the glyph runs (the spike does this: `positioned_glyphs` gives them), which also keeps kerning across words.
4. **Rounded rectangles.** `draw_rect` has no corner radius; the pill and cards need one (`draw_rrect`, or a radius on `draw_rect`). Vello and `vello_cpu` both take a path, so a radius is easy.
5. **Clip and scale.** `push_clip(rect)` in frame pixels is enough for the karaoke fill (the spike clips with the word's own scaled transform; `caption.rs` already computes the scaled rect). A painter should also take a transform (or at least `scale about centre`) per text run, as `TextRun.scale` does now.
6. **Assets** (`draw_asset` for images and video frames) were not tried. They need a texture or pixmap upload per frame, and that is the part of the speed question still open.
7. **Where the code lives.** Vello and Parley should not go into `crastu-render` (a feature crate with a 2,000-line budget). A small engine crate (for example `crastu-gfx`, no dependency on `crastu-render`) holding the Parley wrapper, the two Vello canvases and the readback, and a thin adapter in `crastu-render` that implements `Painter` and `TextLayout` over it. The traits stay where they are, and the layering rule (features depend on engines, never the other way) holds.
8. **The frame key must name the backend.** `frame_key` hashes `RENDER_VERSION`, frame size, `layout.cache_id()`, `t` and the active layers. Since the two renderers differ in 18% of the pixels of a frame, a cached GPU frame must never be served as a CPU frame: the backend (and the library versions, and for `vello_cpu` the SIMD level if goldens are shared) goes into `cache_id()` or the key.

## 5. Dependencies, licences, mobile targets

- `cargo deny --manifest-path spikes/render-gpu/Cargo.toml check licenses bans sources` with the repo's `deny.toml`: `bans ok, licenses ok, sources ok`. `check advisories`: `advisories ok`. Warnings only: duplicate versions (`hashbrown`, `miniz_oxide`, `syn`) and unused licence allowances. Run on the spike's own lockfile, so it shows the tree of these crates only, not of the workspace together with GPUI (whose `wgpu` may be a different version on Linux; media-pipeline.md notes this).
- Size of the tree (unique crates from `cargo tree -e normal`): `vello` 87, `vello_cpu` 57, `parley` 51, the whole spike 140. The `vello_cpu` count excludes wgpu, naga and the shader build; the spike as built still depends on the `vello` crate for re-exported types (kurbo, peniko, `Glyph`), so a production CPU engine crate would depend on `vello_cpu`, `kurbo` and `peniko` directly.
- `cargo check --release --target <t>` passed for `aarch64-apple-ios`, `aarch64-linux-android`, `armv7-linux-androideabi` and `i686-linux-android` with Vello, `vello_cpu` (with `multithreading`) and Parley (no system backend). That checks that the code compiles; it did not link, run, or exercise wgpu on Android (Vulkan or GL) or iOS (Metal), and it did not go through `cargo xtask mobile-check`. Needs hands-on on a device.

## Not done

- M1 timing, a cold shader cache, long-run memory growth, video and image layers, blur, and heavier compositions.
- Root cause of the one-level GPU flip (a Vello or wgpu issue could be filed with the spike as a reproduction: scaled glyph runs, `AaConfig::Area`, frame 1140 here).
- Cross-machine determinism (another Mac, another GPU, Windows, Linux): one machine only. The CPU renderer is the likelier candidate for it, not shown.
- Why the CPU and GPU glyph edges differ.
- `vello_cpu` chooses its SIMD level at run time (`RenderSettings::level`, auto-detected); bytes from another CPU family (for example x86 on CI against NEON here) were not compared, and a golden-file test would need the level pinned or goldens per machine.
- The `Painter` and `TextLayout` changes were not made in `crastu-render`; this is a note on what they would be.

## Decision proposal

Decision 41 (proposed): `crastu-render` slice 2 uses Parley for text layout and shaping with fonts registered from bytes (no system font discovery), and **`vello_cpu` as the reference renderer** for export, golden tests and the first preview, because it was order-independent and identical across the two processes compared here (forward against reverse in one process; frame 20 across two processes with 0 and 4 threads) and rendered a 1080x1920 caption frame in about 2.6 ms including scene building (one noisy run); the Vello GPU renderer (wgpu, Metal) stays an optional backend for preview, added only if a measured heavy composition needs it, and its output is tested with a tolerance (it flipped one pixel by one level in 1 to 2 of 120 frames once glyphs were scaled; the size of the flip was printed for two frames only). The two renderers do not match each other byte for byte (18% of pixels in one frame), so one clip is never previewed on one and exported on the other. The traits change as in section 4: a font reference and shaped glyph runs in `TextRun`, interior mutability or `&mut` for measuring, a corner radius, and the backends live in a new engine crate. Open before slice 2 starts: a heavy-composition measurement (video layer, blur) on the CPU path, an M1 run, and a decision on tolerance against root-causing the GPU flip.
