# Rust ecosystem research: GPUI video editor + TTS/voice-cloning studio + dictation

Verified on: 2026-11-10 (all crate data from `https://crates.io/api/v1/crates/<name>`; last-commit dates from shallow `git clone` of default branch; READMEs/source via raw.githubusercontent.com).
Access limits during research: GitHub REST API, docs.rs, codeberg.org and gitlab.freedesktop.org were blocked by the proxy, so star counts are absent and a few last-commit dates fall back to crates.io release dates (marked).

Legend: ver / release date / license / last commit.

---

## 1. GPUI

| Crate | Latest | Released | License | Last commit | Notes |
|---|---|---|---|---|---|
| `gpui` (official, Zed) | 0.2.2 | 2025-10-22 | Apache-2.0 | zed main 2026-11-09 | Owners: maxbrunsfeld, mikayla-maki, MrSubidubi. Releases: 0.1.0 (2022), 0.2.0 (2025-10-09), 0.2.1 (10-14), 0.2.2 (10-22). **Nothing published to crates.io for ~13 months.** |
| `gpui-pre` / `gpui-pre-platform` | 0.3.12 | 2026-11-09 | Apache-2.0 | — | Weekly snapshots of Zed main ("snapshot of zed@d2a4fb6"), published by `huacnlee` (Longbridge), started 2026-09-03; 13 versions. |
| `gpui-kit` | 0.9.1 | 2026-11-09 | Apache-2.0 | 2026-11-10 | Umbrella crate: re-exports GPUI + gpui-base + gpui-component + assets. Weekly cadence. |
| `gpui-component` (a.k.a. `gpui_component`) | 0.9.1 | 2026-11-09 | Apache-2.0 | 2026-11-10 | Repo now `longbridge/gpui-kit` (old `longbridge/gpui-component` URL redirects). 38 versions. |
| `gpui-base` | 0.9.1 | 2026-11-09 | Apache-2.0 | — | Unstyled behavior layer. |
| `gpui-shell` | 0.9.1 | 2026-11-09 | Apache-2.0 | — | JS extension runtime hosted by Rust. |
| `gpui-fast` | 0.1.14 | 2026-11-09 | Apache-2.0 | — | Longbridge fork with rendering/layout optimizations, opt-in via `gpui-kit` feature. |
| `gpui-ce` | 0.3.0 | 2026-10-20 | Apache-2.0 | — | "Community fork of Zed's GPUI", "mostly API compatible with upstream, but that is changing". |
| `gpui-video-player` | 0.1.3 | 2026-10-23 | MIT | — | GStreamer-based video element; supports `gpui` 0.2 or `gpui-pre` via features. |
| `alhazen-gpui` | 0.7.1 | 2026-11-02 | Apache-2.0 | — | "Video player for GPUI apps, built on the alhazen-core media engine"; 125 downloads, very new. |

Sources:
- https://crates.io/api/v1/crates/gpui , .../gpui/owner_user
- https://crates.io/api/v1/crates/gpui-pre , gpui-pre-platform, gpui-kit, gpui-component, gpui-base, gpui-shell, gpui-fast, gpui-ce, gpui-video-player, alhazen-gpui
- https://raw.githubusercontent.com/zed-industries/zed/main/crates/gpui/Cargo.toml (version = "0.2.2", publish = true, license Apache-2.0)
- https://raw.githubusercontent.com/zed-industries/zed/main/crates/gpui/README.md (pre-1.0, "There will often be breaking changes"; now tells users to add `gpui` + `gpui_platform`)
- https://raw.githubusercontent.com/longbridge/gpui-component/main/README.md and .../Cargo.toml
- https://raw.githubusercontent.com/gpui-ce/gpui-ce/main/README.md
- https://raw.githubusercontent.com/cijiugechu/gpui-video-player/main/README.md

### Key facts
- Zed's gpui source in-repo is still `0.2.2` with `publish = true`, but the workspace default is `publish = false`, and the new split crates (`gpui_platform`, `gpui_macos`, `gpui_linux`, `gpui_windows`, `gpui_wgpu`, `gpui_web`) are **not on crates.io** (`gpui_platform` returns not-found). In practice the README's "`gpui_platform = *`" instructions only work through a git dependency or the `gpui-pre-*` snapshots. Source: zed root Cargo.toml `[workspace.package] publish = false`; crates.io 404 for gpui_platform.
- gpui-component pins `gpui = { package = "gpui-pre", version = "=0.3.12" }`. The Cargo.toml comment says any snapshot may change GPUI's API, and the crates must be bumped together. **Practical recommendation: depend on `gpui-kit` (or gpui-component + exact-pinned gpui-pre) and upgrade in lockstep weekly, or vendor a Zed git rev.**
- gpui-component features (README): 75+ components; Dock layout (resizable panels, draggable tabs, nested splits, edge docks, serializable); virtual lists (variable heights); data tables (virtual scrolling, fixed/resizable columns, sorting, cell selection, "hundreds of thousands of rows"); code editor (Tree-sitter + LSP, 200K lines); semantic themes; Markdown/HTML rendering; charts; AccessKit a11y; headless UI testing; wasm32 support; macOS/Windows/Linux.
- Who ships GPUI apps: Zed (obviously); Longbridge Pro ("GPUI Kit has powered Longbridge Pro from day one", gpui-kit README). OpenCut's rewrite desktop app uses `gpui = "0.2.2"` (see section 5). **Discord shipping GPUI: UNVERIFIED** (no primary source found).

### Rendering backends (Zed main, 2026-11-10)
- macOS: Metal (`gpui_macos`). Linux: `gpui_wgpu` using wgpu (vulkan, gles, metal, webgpu features; git rev of wgpu). Windows: Direct3D11 + DirectComposition (`gpui_windows` features `Win32_Graphics_Direct3D11`, `Win32_Graphics_DirectComposition`). Sources: zed `crates/gpui_linux/Cargo.toml`, `crates/gpui_wgpu/Cargo.toml`, `crates/gpui_windows/Cargo.toml`.

### Video frames into GPUI (limitations)
- `img()` accepts `ImageSource::Render(Arc<RenderImage>)` and `ImageSource::Custom(closure -> Arc<RenderImage>)` (zed `crates/gpui/src/elements/img.rs`). This is a CPU BGRA buffer uploaded to the sprite atlas, which works on all platforms but costs a CPU copy and upload per frame. Fine for preview at moderate resolutions; poor for 4K multi-track scrubbing.
- `surface()` exists **only on macOS/iOS** and takes `CFRetained<CVPixelBuffer>` (`SurfaceSource::Surface`), so hardware-decoded VideoToolbox frames are zero-copy (zed `crates/gpui/src/elements/surface.rs`). There is no equivalent on Linux or Windows (the enum has no other variants).
- Implication: for a cross-platform NLE viewer, either (a) composite on your own wgpu device and read back to RenderImage (simple, costs a copy), (b) on macOS hand CVPixelBuffer/IOSurface to `surface()`, or (c) patch or fork GPUI to expose a shared-texture element on wgpu/D3D11. The existing GPUI video players (`gpui-video-player` on GStreamer, `alhazen-gpui`) show the pattern; inspect their source before choosing.

---

## 2. Speech-to-text in Rust

| Crate | Latest | Released | License | Last commit | Notes |
|---|---|---|---|---|---|
| `whisper-rs` | 0.16.0 | 2026-03-12 | Unlicense | n/a (repo moved to codeberg.org/tazz4843/whisper-rs, which was blocked) | whisper.cpp bindings. ~8 months since last release. |
| `sherpa-onnx` (official) | 2.0.1 | 2026-10-27 | Apache-2.0 | k2-fsa/sherpa-onnx 2026-11-10 | **Official Rust API** from k2-fsa, plus `sherpa-onnx-sys` 2.0.1. Frequent releases (1.13.3 → 2.0.1 between Jun and Oct 2026). |
| `sherpa-rs` | 0.6.8 | 2025-10-05 | MIT | 2026-03-08 | **Deprecated**: README says to use the official sherpa-onnx Rust bindings. |
| `ort` | 2.0.0-rc.15 | 2026-10-19 | MIT OR Apache-2.0 | 2026-11-01 | Bindings to ONNX Runtime 1.31 (ort-sys description). Still release-candidate; MSRV 1.92 as of rc.14. 25.6M downloads. |
| `candle-core` / `candle-transformers` | 0.11.1 | 2026-10-31 | MIT OR Apache-2.0 | 2026-11-01 | Examples include `whisper`, `whisper-microphone`, `voxtral`, plus TTS `csm`, `parler-tts`, `metavoice`, `mimi`. |
| `transcribe-rs` | 0.3.12 | 2026-10-02 | MIT | 2026-10-03 | cjpais. Features: `onnx` (Parakeet, Canary, Cohere, Moonshine, SenseVoice, GigaAM via ORT), `whisper-cpp` (Metal/Vulkan/CUDA), `whisperfile`; ORT EPs cuda/rocm/directml/coreml/webgpu. |
| `transcribe-cpp` | 0.4.1 | 2026-10-22 | MIT | — | Bindings to handy-computer/transcribe.cpp (ggml-based STT). **Handy now uses this, not transcribe-rs.** |
| `parakeet-rs` | 0.4.4 | 2026-10-31 | MIT OR Apache-2.0 | 2026-10-31 | Parakeet via ort. |

The official sherpa-onnx Rust examples (`rust-api-examples/examples/`) cover ASR for whisper, nemo_parakeet, parakeet_tdt (simulated streaming mic), moonshine_v2, qwen3_asr, sense_voice, cohere_transcribe, fire_red_asr, funasr_nano, streaming zipformer/paraformer, punctuation, speaker diarization and embeddings, spoken language ID, speech enhancement (gtcrn, dpdfnet, voxlock), keyword spotting, plus **silero and TEN VAD**.
- `sherpa-onnx-sys/build.rs` downloads prebuilt libs from GitHub releases by default, with `static` and `shared` features and `SHERPA_ONNX_LIB_DIR` / `SHERPA_ONNX_ARCHIVE_DIR` overrides. Offline/CI builds should set these.

### VAD
| Crate | Latest | Released | License | Notes |
|---|---|---|---|---|
| `voice_activity_detector` | 0.2.1 | 2025-08-04 | MIT (LICENSE file; crates.io shows "non-standard" because it uses `license-file`) | Silero via ort; last commit 2025-08-04. |
| `earshot` (pykeio) | 1.2.2 | 2026-08-19 | MIT OR Apache-2.0 | "Ridiculously fast & accurate VAD in pure Rust"; no ONNX dependency. Used by Handy. |
| `silero-vad-rs` | 0.1.2 | 2025-04-04 | MIT | 3k downloads; avoid. |
| sherpa-onnx | — | — | — | Silero and TEN VAD built in. |

Sources: crates.io API for each; https://raw.githubusercontent.com/thewh1teagle/sherpa-rs/main/README.md ; https://raw.githubusercontent.com/cjpais/transcribe-rs/main/README.md ; https://raw.githubusercontent.com/k2-fsa/sherpa-onnx/master/sherpa-onnx/rust/sherpa-onnx-sys/build.rs ; git tree of k2-fsa/sherpa-onnx rust-api-examples ; git tree of huggingface/candle candle-examples ; https://raw.githubusercontent.com/nkeenan38/voice_activity_detector/main/LICENSE

---

## 3. TTS and voice cloning in Rust

| Option | Version / date | License | Status |
|---|---|---|---|
| sherpa-onnx TTS (official crate) | 2.0.1 / 2026-10-27 | Apache-2.0 (code) | Rust examples: kokoro (en, zh+en), kitten, matcha, vits/piper-style, **supertonic_tts**, **pocket_tts (zero-shot cloning from a reference wav)**, **zipvoice (zero-shot cloning with reference audio + reference text)**. |
| Kokoros (lucasjinreal/Kokoros) | not on crates.io as `kokoros` (404) | Apache-2.0 per repo badge (unverified via API) | Last commit 2026-10-05; ort-based; CLI + OpenAI-compatible server; README points to successor project "Crane". Use as a git dep or reference. |
| `kokoro-tts` (mzdk100) | 0.3.3 / 2026-06-01 | Apache-2.0 | Alternative Kokoro crate. |
| `piper-rs` | 0.2.0 / 2026-05-21 | MIT | Last commit 2026-05-21; depends on `espeak-rs` (espeak-ng). |
| Supertonic | upstream archived | sample code MIT; model OpenRAIL-M | supertone-inc/supertonic README: "This repository is archived. Development and support have ended." Weights moved to HF `supertone-oss-archive`. Still runnable through sherpa-onnx's `supertonic_tts` example. |
| candle | 0.11.1 | MIT/Apache | csm, parler-tts, metavoice examples (heavier, GPU-oriented). |

Model licenses: Kokoro-82M weights Apache-2.0 (https://huggingface.co/hexgrad/Kokoro-82M/raw/main/README.md). Piper upstream (`rhasspy/piper`) says "Development has moved: https://github.com/OHF-Voice/piper1-gpl" (new home is GPL-named; LICENSE not fetched).

**Licensing risk:** espeak-ng is GPL-3.0 (https://raw.githubusercontent.com/espeak-ng/espeak-ng/master/COPYING). Kokoro, Piper and VITS phonemization paths that link espeak-ng (piper-rs via espeak-rs, Kokoros' espeak tokenizer, sherpa-onnx "phonemize" variants) bring GPL obligations if statically linked. Prefer "lexicon" variants or a non-GPL G2P, or isolate espeak in a sidecar process. Which sherpa-onnx builds link espeak-ng: **unverified**.

Voice cloning:
- **In-process Rust, available now:** sherpa-onnx Pocket TTS and ZipVoice (both take reference audio; verified in `pocket_tts.rs` / `zipvoice_tts_lexicon.rs`). Pocket TTS model license: **unverified** (HF README fetch failed).
- **Python sidecar likely needed for:** state-of-the-art cloning stacks (F5-TTS, XTTS, CosyVoice, Chatterbox, etc.) that have no maintained ONNX/Rust port. This was not verified per model in this pass. Design a sidecar protocol (stdin/stdout JSON or local gRPC) so these can be added later.

---

## 4. Audio I/O

| Crate | Latest | Released | License | Last commit | Notes |
|---|---|---|---|---|---|
| `cpal` | 0.18.2 | 2026-08-16 | Apache-2.0 | 2026-11-02 | 0.18.0 (2026-06-06) added a **native PipeWire host**. CoreAudio loopback (system audio) added in 0.17.0 (2025-12-20), needs macOS 14.6+. WASAPI loopback: `AUDCLNT_STREAMFLAGS_LOOPBACK` set in `src/host/wasapi/device.rs` (input stream on an output device). Hosts: ALSA/JACK/PipeWire/PulseAudio, CoreAudio/JACK, WASAPI/ASIO/JACK. |
| `rodio` | 0.23.0 | 2026-10-11 | MIT OR Apache-2.0 | 2026-09-07 | Playback. |
| `rubato` | 5.0.1 | 2026-10-01 | MIT OR Apache-2.0 | 2026-10-01 | Resampling (Handy uses `rubato = "5"`). |
| `symphonia` | 0.6.1 | 2026-08-13 | MPL-2.0 | 2026-11-08 | Pure-Rust decode (Handy still pins 0.5.5). |
| `screencapturekit` | 11.1.2 | 2026-11-02 | MIT OR Apache-2.0 | 2026-11-02 | doom-fish. macOS 12.3+; system audio (13.0+) and microphone (15.0+); zero-copy IOSurface/Metal frames; recording output (15.0+). |
| `wasapi` | 0.27.0 | 2026-11-06 | MIT | 2026-11-06 | HEnquist; direct WASAPI including loopback. |
| `pipewire` | 0.10.1 | 2026-08-19 | MIT | n/a (freedesktop GitLab blocked) | Official pipewire-rs bindings. |
| `handy-recorder` | 0.1.0 | 2026-10-02 | MIT | — | "Cross-platform microphone capture: exact, ordered, framed audio on an ordinary thread" (Handy's recorder). |

Sources: crates.io API; https://raw.githubusercontent.com/RustAudio/cpal/master/CHANGELOG.md and README.md; cpal source `src/host/wasapi/device.rs`; https://raw.githubusercontent.com/doom-fish/screencapturekit-rs/main/README.md

---

## 5. Video

| Crate | Latest | Released | License | Last commit | Notes |
|---|---|---|---|---|---|
| `ffmpeg-next` | 9.0.0 | 2026-08-05 | WTFPL | 2026-10-09 | README: "**maintenance-only mode**"; new FFmpeg APIs need outside PRs, and improvements to the existing API are "unlikely to be merged". |
| `ffmpeg-sys-next` | 9.0.1 | 2026-10-09 | WTFPL | — | Raw FFI. |
| `rsmpeg` | 0.18.0+ffmpeg.8.0 | 2025-08-24 | MIT | 2025-08-24 | larksuite; no commits for 14+ months. |
| `gstreamer` (+ `gstreamer-video`) | 0.25.4 | 2026-09-21 | MIT OR Apache-2.0 (bindings) | — | Official bindings; 128 versions. GStreamer itself is LGPL; codec plugins vary. |
| `wgpu` | 30.0.1 | 2026-08-22 | MIT OR Apache-2.0 | 2026-11-09 | Zed's Linux renderer uses a wgpu git rev, so there may be a version mismatch if you share a device with GPUI. |

LGPL: link FFmpeg **dynamically**, build without `--enable-gpl`/`--enable-nonfree`, and ship the license and build flags. This is exactly OpenCut's approach: `crates/media/setup/ffmpeg.json` pins FFmpeg 8.1.3, `"license": "LGPL-2.1-or-later"`, BtbN `lgpl-shared` builds for win/linux x64+arm64, and the workflow notes say "Shared LGPL builds ... Each archive holds the exact configure flags (BUILD.md) and FFmpeg's licence." (https://raw.githubusercontent.com/OpenCut-app/OpenCut/main/crates/media/setup/ffmpeg.json , .github/workflows/media-deps.yml). H.264/HEVC encoding without GPL x264/x265 means relying on platform encoders (VideoToolbox, Media Foundation, VA-API/NVENC). This is a general fact and was not re-verified here.

OpenCut rewrite (https://raw.githubusercontent.com/OpenCut-app/OpenCut/main/README.md): "being rewritten from the ground up": Rust core, plugin-first architecture, Editor API, MCP server, headless mode, scripting tab; desktop + mobile + browser from one codebase. `apps/desktop` uses `gpui = "0.2.2"` and its README says "Very early. Right now this is just a window that opens." The `crates/media` crate is still commented out of the workspace. Last commit 2026-09-24. Classic version lives at opencut-app/opencut-classic. Conclusion: OpenCut is a useful signal (GPUI + shared LGPL FFmpeg) but has no reusable NLE engine yet.

Other Rust NLE projects: not surveyed in depth (**unverified**).

---

## 6. System-wide dictation

| Crate | Latest | Released | License | Last commit | Platform notes |
|---|---|---|---|---|---|
| `global-hotkey` (tauri) | 0.10.0 | 2026-11-09 | Apache-2.0 OR MIT | 2026-11-09 | Windows, macOS, **Linux X11 only**; macOS manager must live on the main-thread event loop. |
| `rdev` | 0.5.3 | **2023-06-26** | MIT | 2026-05-12 (Narsil repo) | `listen` is X11-only; `unstable_grab` uses evdev (X11+Wayland). Handy uses the fork `rustdesk-org/rdev` via git. |
| `handy-keys` | 0.3.4 | 2026-08-07 | MIT | 2026-08-07 | macOS/Windows/Linux; modifier-only hotkeys; blocking; hotkey recording. Linux reads evdev directly (works on Wayland/X11/console) and needs `/dev/input` read access; blocking needs `/dev/uinput`. For shipping, use a udev `uaccess` rule. macOS needs Accessibility permission (helpers provided). |
| `enigo` | 0.6.1 | 2025-08-28 | MIT | 2026-11-07 | Win/macOS/X11 stable; Wayland and libei input are "Experimental" behind feature flags. |
| `arboard` | 3.7.0 | 2026-10-15 | MIT OR Apache-2.0 | 2026-11-02 | 1Password; enable `wayland-data-control` for Wayland. |
| `ashpd` | 0.14.0 | 2026-11-05 | MIT | 2026-11-08 | xdg-desktop-portal client: has `global_shortcuts`, `remote_desktop`, `input_capture`, `screencast`, `clipboard` modules. |

Wayland reality (from Handy, https://raw.githubusercontent.com/cjpais/Handy/main/src-tauri/src/clipboard.rs and README.md): to paste, Handy tries `wtype` first, but not on KDE or GNOME, because neither implements `zwp_virtual_keyboard_manager_v1`. It then tries `dotool` and `ydotool`, and falls back to enigo. The README says Ubuntu 26.04 Wayland needs ydotool + systemd setup, and that on Wayland global shortcuts must be configured in the DE, which calls Handy's CLI flags. The portal route (GlobalShortcuts + RemoteDesktop keyboard via ashpd) is the sanctioned alternative; per-DE support status is **unverified**.

### Handy (cjpais/Handy), last commit 2026-11-06. Crates from src-tauri/Cargo.toml:
tauri 2.11 (+ plugins: global-shortcut, clipboard-manager, macos-permissions, autostart, single-instance, updater, store, fs, dialog, os, process, log, opener), `rdev` (git rustdesk-org fork), `handy-keys 0.3.4`, `enigo 0.6.1`, `handy-recorder 0.1`, `earshot 1.2.2` (VAD; there is also a `vad/silero.rs` module), `transcribe-cpp 0.4` ("Every model runs through transcribe-cpp (GGUF/ggml, native backends)"), `rodio 0.23` (feedback sounds only), `symphonia 0.5.5`, `rubato 5`, `hound`, `rustfft`, `rusqlite 0.37`, `hf-hub` (git fork), `reqwest`, `whatlang`, `ferrous-opencc`, `specta`/`tauri-specta`. It has an `engine_supervisor` (worker process) module, so the inference engine is supervised separately. Source: https://raw.githubusercontent.com/cjpais/Handy/main/src-tauri/Cargo.toml and repo tree.

---

## 7. Plugin/module loading

| Option | Crate / version | License | Maintenance | Notes |
|---|---|---|---|---|
| Cargo features (compile-time) | — | — | — | Simplest; no ABI issues. Fits "modules" that ship in-tree. |
| `libloading` | 0.9.0 / 2025-11-05 | ISC | — | Raw dylib loading; Rust ABI is unstable, so you need C ABI or one of the next two. |
| `abi_stable` | 0.11.3 / **2023-10-12** | MIT/Apache-2.0 | last commit 2023-10-12 | Effectively dormant for 3 years. |
| `stabby` | 72.1.16 / 2026-07-20 | EPL-2.0 OR Apache-2.0 | last commit 2026-09-27 | Active (ZettaScale); niche-optimized stable ABI; `#[stabby::export]`/import with layout checks. |
| `wasmtime` | 50.0.1 / 2026-11-05 | Apache-2.0 WITH LLVM-exception | 2026-11-09 | Component model + WIT; `wit-bindgen` 0.63.0 (2026-10-14). |
| `extism` | 1.41.0 / 2026-10-09 | BSD-3-Clause | 2026-10-09 (upgraded to wasmtime v49) | Core-wasm plugin framework with PDKs; README highlights "host-controlled HTTP without WASI". Not component-model based. |
| `gpui-shell` | 0.9.1 | Apache-2.0 | 2026-11-09 | JS extension runtime for GPUI apps (Longbridge). |
| Out-of-process sidecar | — | — | — | Needed for Python ML (voice cloning), and isolates GPL tools (espeak-ng) and crashes. Handy already uses a supervised engine worker. |

Zed extensions: `crates/extension_host` uses `wasmtime` + `wasmtime-wasi` 48.0.7 (`p2` feature, component model). `zed_extension_api` (crates.io 0.7.0, 2025-09-12; in-repo 0.8.0) uses `wit-bindgen 0.41` with a `wit/` directory versioned as `since_v*`. Sources: https://raw.githubusercontent.com/zed-industries/zed/main/Cargo.toml , crates/extension_host/Cargo.toml , crates/extension_api/Cargo.toml ; https://crates.io/api/v1/crates/zed_extension_api

---

## Recommendations (summary)
1. UI: `gpui-kit` 0.9.x (pins gpui-pre). Upgrade weekly in lockstep. Budget for API churn and possibly a GPUI fork to get a cross-platform GPU texture element.
2. Video preview: decode with LGPL shared FFmpeg (via ffmpeg-sys-next/ffmpeg-next, accepting maintenance-mode risk, or a thin in-house FFI) or GStreamer. Composite in wgpu. Present via `RenderImage` everywhere and `surface()` on macOS.
3. Speech: official `sherpa-onnx` crate as the single engine for STT (Parakeet/Whisper/Moonshine), VAD (Silero/TEN), TTS (Kokoro/Kitten/Matcha) and in-process cloning (Pocket TTS, ZipVoice). Keep `ort` 2.0-rc for custom ONNX models. Consider `transcribe-cpp`/whisper-rs for GGML/Metal paths.
4. Audio: cpal 0.18 (now covers PipeWire and loopback on all 3 OSes) + rubato + symphonia; `screencapturekit` for macOS app-audio capture.
5. Dictation: `handy-keys` for hotkeys (works on Wayland via evdev), `arboard` with `wayland-data-control`, `enigo` + external wtype/dotool/ydotool fallbacks on Wayland, and ashpd portals as the long-term path.
6. Extensibility: cargo features for first-party modules; wasmtime component model (Zed's approach) for third-party plugins; sidecar processes for Python and GPL tools. Avoid `abi_stable` (dormant); use `stabby` only if native dylib plugins are unavoidable.

## Unverified / gaps
- Discord shipping GPUI; whisper-rs last commit (codeberg blocked); pipewire-rs last commit (GitLab blocked); Kokoros license; Pocket TTS model license; which sherpa-onnx TTS builds link espeak-ng; per-DE support for the GlobalShortcuts and RemoteDesktop portals; Python-only cloning models; other Rust NLE projects.
