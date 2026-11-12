# Vendored GPUI patch: YCbCr matrix per surface

Part of issue #39. Evidence: [`docs/spikes/gpui-video.md`](spikes/gpui-video.md), section "Colour matrix measurement".

## Why

GPUI's `surface()` draws a `CVPixelBuffer` through `surface_fragment` in `gpui-pre-apple`, which converts NV12 with a BT.601 full-range matrix written into the shader. Camera and phone video is BT.709 video range, so every colour is off (CIEDE2000 up to 8.0, limit 1.0). The editor's preview has to match the export (decision 27), so Crastu carries a small patch until upstream can choose the matrix per surface.

## What is patched

`vendor/gpui-pre-apple/` is a copy of `gpui-pre-apple` 0.3.12 (Zed rev `d2a4fb6`, Apache-2.0) with four files changed: about 107 changed lines, listed in `vendor/gpui-pre-apple.patch` (`diff -ruN` against the published crate, so a bump can re-apply it with `patch -p1`).

| File | Change |
|---|---|
| `src/ycbcr.rs` (new, no dependencies) | `Matrix` (BT.601, BT.709, BT.2020), `Matrix::from_cv_name` for the CoreVideo tag, `transform(matrix, full_range)` returning the 4x4 matrix the shader multiplies by |
| `src/shaders.metal` | `surface_fragment` takes the matrix as a buffer argument instead of a constant |
| `src/metal_renderer.rs` | per surface: read `kCVImageBufferYCbCrMatrixKey` from the buffer, pick full or video range from the pixel format (`420f` as before, `420v` is now accepted), upload the matrix with `setFragmentBytes`; a new `SurfaceInputIndex::YCbCrToRgb` slot |
| `src/gpui_apple.rs` | `mod ycbcr;` |

No change to `gpui-pre` and no new public API: the colour information travels on the `CVPixelBuffer` itself, so `surface(buffer)` keeps its signature. A buffer without a usable tag is decoded as BT.601, exactly as before.

Wiring: `[patch.crates-io] gpui-pre-apple = { path = "vendor/gpui-pre-apple" }` in the root `Cargo.toml`, and `exclude = ["vendor"]` so the copy is not a workspace member (no Crastu lints, layering or size checks apply to it). `gpui-component` and `gpui-pre-platform` keep their `=0.3.12` pins, which the copy satisfies.

Licences: the copy is Apache-2.0 like Crastu. Apache-2.0 section 4(b): each of the four changed files starts with the line `Modified by Crastu: per-surface YCbCr matrix, issue #39 (see docs/vendor-gpui.md).` The crate's own `LICENSE-APACHE` (the unmodified standard text, no copyright line) is **omitted** from the copy, because Cora flags the licence's plain-HTTP link (`sec-hardcoded-url`) in any added file and its ignore list does not stop that rule; the full text is in `LICENSE` and `LICENSES/`, and the `NOTICE` entry for `gpui-pre-apple` says so. `cargo xtask licences` lists the copy in `NOTICE` (it treats a crate under `vendor/` as third-party), and `cargo deny check` sees it as the same package and version.

Diffs and review: `.gitattributes` marks `vendor/gpui-pre-apple/**` as `-diff eol=lf`, so the 27k lines of third-party source do not appear in `git diff`; `.cora.yaml` ignores the same glob. The change itself is reviewed through `vendor/gpui-pre-apple.patch`, which sits outside the glob and stays diffable.

Verify that the patch reproduces the copy exactly (from the repository root; the pristine crate is in the cargo registry):

```sh
P=$(ls -d ~/.cargo/registry/src/*/gpui-pre-apple-0.3.12)
rm -rf /tmp/vchk && cp -R "$P" /tmp/vchk
(cd /tmp/vchk && patch -p1 < "$OLDPWD/vendor/gpui-pre-apple.patch")
diff -r --exclude=.cargo-ok --exclude=Cargo.lock --exclude=Cargo.toml.orig --exclude=LICENSE-APACHE /tmp/vchk vendor/gpui-pre-apple && echo reproduces
```

To regenerate the patch after an edit: `diff -ruN` with the same four excludes, pristine crate first, then rewrite the paths to `a/` and `b/`.

## What the media layer must do

The patch reads the tag, it does not guess. The code that hands a decoded buffer to `surface()` must make sure the buffer carries `kCVImageBufferYCbCrMatrixKey` (`ITU_R_709_2`, `ITU_R_601_4` or `ITU_R_2020`), from the stream's colour metadata, with `CVBufferSetAttachment` where the decoder did not set it. When the stream carries no colour metadata, the usual convention applies (601 below 720 lines, 709 from 720 up). Whether the buffers that VideoToolbox returns through FFmpeg already carry the tag was not checked here: verify it in the first slice that plays real footage, and set it explicitly if not.

## How the GPU check was made

The committed check is `cargo test -p xtask-colour` (see the spike doc). The GPU part stays a documented manual check: a test in `xtask/colour` that renders through the vendored renderer would need `gpui-pre-apple` with `test-support` as a dependency of a `tool` crate (`cargo xtask deps-check` allows GPUI only in ui, feature-ui and bin crates), `unsafe` for the CoreVideo buffer writes (denied workspace-wide), and a Metal device on CI. Instead the recorded read-backs are tested against the model (within one 8-bit level per channel). The four files in `xtask/colour/fixtures/` are read-backs from a real Metal render, made with throwaway code that is not committed:

1. Copy the crate to a scratch directory (once the published one, once `vendor/gpui-pre-apple`), add the crate's own `Cargo.lock` and `"CVBase"` to the `objc2-core-video` features (the crate's unit tests do not build without it).
2. Add a test beside `render_core_video_surface` in `metal_renderer.rs`: read the chart's samples (`cargo xtask colour-report --chart`), write them into a 96 by 64 IOSurface-backed `CVPixelBuffer` (one 16 by 16 block per patch, chroma at half size), set the matrix tag, put the buffer in a `PaintSurface`, call `MetalRenderer::new_headless(..).render_scene_to_image(..)` and print the pixel at each block centre as `r g b`.
3. Run `cargo test --features test-support,runtime_shaders` (Xcode 26 has no Metal compiler, decision 32), then `cargo xtask colour-report --captured <file>` to get the table and the verdict.

`Window::render_to_image` was not used: the renderer's own offscreen entry point is enough and needs no window.

## Upstream proposal

To file against `zed-industries/zed` (crate `gpui_apple`, `crates/gpui_apple/src/shaders.metal`, `surface_fragment`):

> **gpui: choose the YCbCr to RGB matrix and range per `surface()`**
>
> `surface_fragment` converts NV12 with a BT.601 full-range matrix that is a constant in the shader. Most HD and UHD video is BT.709 (some BT.2020) and is often video range, so colours are visibly off: on a BT.709 test chart the CIEDE2000 error is up to 8 on saturated colours and about 2 on dark greys, and the renderer rejects `420v` buffers altogether.
>
> Proposal: derive the matrix from the buffer. `kCVImageBufferYCbCrMatrixKey` already says which matrix the producer used, and the pixel format says the range. Build the 4x4 matrix on the CPU per surface, pass it with `setFragmentBytes`, and make the shader multiply by it. About 100 lines in `gpui_apple`, no change to the public `surface()` API; untagged buffers keep today's BT.601 look. A follow-up could add an explicit override on the `surface()` element (an optional colour description) for producers that cannot tag buffers.
>
> We have it working with a read-back test of a colour chart on Apple silicon and can send the patch (`vendor/gpui-pre-apple.patch` in https://github.com/ajianaz/crastu).

Not yet filed: file it, add the link here and in issue #39.

## Exit condition

Delete `vendor/gpui-pre-apple/`, the `[patch.crates-io]` entry, the `exclude`, and the `vendor/` rule in `xtask/licences` when a `gpui-pre` release that Crastu can pin does one of these:

- reads the matrix and range per surface (from the tag or an explicit parameter), and `cargo test -p xtask-colour` still passes against it (point the test at the shipped behaviour then), or
- Crastu replaces `surface()` with its own Metal view for the preview.

When GPUI is bumped for any other reason, re-apply `vendor/gpui-pre-apple.patch` to the new `gpui-pre-apple` first (`patch -p1` inside a fresh copy), re-run the chart check, and keep the pins of `gpui-pre`, `gpui-pre-platform` and `gpui-component` moving together as before.
