# Rust ecosystem: GPUI, dictation, plugin loading

Verified on: 2026-11-10 (all crate data from `https://crates.io/api/v1/crates/<name>`; last-commit dates from shallow `git clone` of default branch; READMEs/source via raw.githubusercontent.com).
Access limits during research: GitHub REST API, docs.rs, codeberg.org and gitlab.freedesktop.org were blocked by the proxy, so star counts are absent and a few last-commit dates fall back to crates.io release dates (marked).

Legend: ver / release date / license / last commit.

Topic notes split out of this survey: [speech.md](speech.md) (speech-to-text, VAD, TTS, voice cloning), [media-pipeline.md](media-pipeline.md) (audio I/O, video, LGPL FFmpeg), [auto-clipper.md](auto-clipper.md), [mcp.md](mcp.md).

## Prior art

Every project consulted in this note, on equal footing. "Concepts only" marks a project whose licence does not allow us to reuse its code; Crastu takes concepts only from all of them (decisions 1 and 35). Licences are as listed in the tables below, checked against crates.io on 2026-11-10, and for the repositories named here against GitHub on 2026-11-13.

| Project | Licence | Concept taken | Link |
|---|---|---|---|
| Zed / GPUI | Apache-2.0 for the `gpui` crate (the zed repository mixes licences) | The UI framework Crastu builds on | https://github.com/zed-industries/zed |
| gpui-component / gpui-kit (Longbridge) | Apache-2.0 | Component set, dock layout, weekly snapshot cadence | https://github.com/longbridge/gpui-component |
| gpui-ce | Apache-2.0 | Community fork of GPUI, for comparison | https://github.com/gpui-ce/gpui-ce |
| Handy (cjpais/Handy) | MIT; concepts only (decision 1 keeps us to concepts) | Dictation crate choices, Wayland paste fallbacks, supervised engine worker | https://github.com/cjpais/Handy |
| OpenCut (OpenCut-app/OpenCut) | MIT; concepts only | GPUI desktop app, shared LGPL FFmpeg | https://github.com/OpenCut-app/OpenCut |
| Zed extension host | Apache-2.0 for the crates named here | wasmtime component-model plugins | https://github.com/zed-industries/zed |
| Plugin-loading crates: libloading, abi_stable, stabby, wasmtime, extism | ISC, MIT/Apache-2.0, EPL-2.0 OR Apache-2.0, Apache-2.0 WITH LLVM-exception, BSD-3-Clause | The options compared in the plugin section below | https://crates.io/crates/wasmtime |
| Hotkey, input and clipboard crates: global-hotkey, rdev, handy-keys, enigo, arboard, ashpd | MIT or Apache-2.0 (see the dictation table) | System-wide dictation building blocks | https://crates.io/crates/handy-keys |

## GPUI

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
- Who ships GPUI apps: Zed (obviously); Longbridge Pro ("GPUI Kit has powered Longbridge Pro from day one", gpui-kit README). OpenCut's rewrite desktop app uses `gpui = "0.2.2"` (see [media-pipeline.md](media-pipeline.md)). **Discord shipping GPUI: UNVERIFIED** (no primary source found).

### Rendering backends (Zed main, 2026-11-10)
- macOS: Metal (`gpui_macos`). Linux: `gpui_wgpu` using wgpu (vulkan, gles, metal, webgpu features; git rev of wgpu). Windows: Direct3D11 + DirectComposition (`gpui_windows` features `Win32_Graphics_Direct3D11`, `Win32_Graphics_DirectComposition`). Sources: zed `crates/gpui_linux/Cargo.toml`, `crates/gpui_wgpu/Cargo.toml`, `crates/gpui_windows/Cargo.toml`.

### Video frames into GPUI (limitations)
- `img()` accepts `ImageSource::Render(Arc<RenderImage>)` and `ImageSource::Custom(closure -> Arc<RenderImage>)` (zed `crates/gpui/src/elements/img.rs`). This is a CPU BGRA buffer uploaded to the sprite atlas, which works on all platforms but costs a CPU copy and upload per frame. Fine for preview at moderate resolutions; poor for 4K multi-track scrubbing.
- `surface()` exists **only on macOS/iOS** and takes `CFRetained<CVPixelBuffer>` (`SurfaceSource::Surface`), so hardware-decoded VideoToolbox frames are zero-copy (zed `crates/gpui/src/elements/surface.rs`). There is no equivalent on Linux or Windows (the enum has no other variants).
- Implication: for a cross-platform NLE viewer, either (a) composite on your own wgpu device and read back to RenderImage (simple, costs a copy), (b) on macOS hand CVPixelBuffer/IOSurface to `surface()`, or (c) patch or fork GPUI to expose a shared-texture element on wgpu/D3D11. The existing GPUI video players (`gpui-video-player` on GStreamer, `alhazen-gpui`) show the pattern; inspect their source before choosing.

### Alternatives to GPUI

Added 2026-11-13 from the GitHub and crates.io metadata of each project; they were not evaluated beyond that and none of them was built or benchmarked.

| Project | Licence | Latest on crates.io | Last push | Notes |
|---|---|---|---|---|
| egui (emilk/egui) | Apache-2.0 | 0.37.0 (2026-10-15) | 2026-11-12 | Immediate-mode GUI, runs on web and native |
| iced (iced-rs/iced) | MIT | 0.14.0 (2025-12-07) | 2026-11-12 | Elm-inspired GUI library |
| Slint (slint-ui/slint) | Several options; GitHub reports no single SPDX id, so read the repository's licence files before any use | 1.18.1 (2026-09-21) | 2026-11-13 | Declarative toolkit for Rust, C++, JavaScript and Python |
| Xilem (linebender/xilem) | Apache-2.0 | 0.4.0 (2025-10-29) | 2026-11-03 | Described by its own repository as an experimental Rust native UI framework |
| Dioxus (DioxusLabs/dioxus) | Apache-2.0 | 0.8.0-alpha.3 (2026-10-16) | 2026-11-11 | Fullstack framework for web, desktop and mobile |
| Floem (lapce/floem) | MIT | 0.2.0 (2024-11-14) | 2026-09-24 | Native UI library with fine-grained reactivity |
| Makepad (makepad/makepad) | MIT | `makepad-widgets` 1.0.0 (2025-05-13) | 2026-11-12 | Compiles to wasm/WebGL, Metal, DirectX 11 and OpenGL |
| Tauri (tauri-apps/tauri) | Apache-2.0 | not checked | 2026-11-12 | Web front end in a native shell; Handy uses it (see the dictation section) |

[ARCHITECTURE.md](../ARCHITECTURE.md) records why GPUI is the front end; this table is only the list of what else exists.

## System-wide dictation

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

## Plugin and module loading

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

## Recommendations (summary)
The speech, audio and video items refer to [speech.md](speech.md) and [media-pipeline.md](media-pipeline.md).

1. UI: `gpui-kit` 0.9.x (pins gpui-pre). Upgrade weekly in lockstep. Budget for API churn and possibly a GPUI fork to get a cross-platform GPU texture element.
2. Video preview: decode with LGPL shared FFmpeg (via ffmpeg-sys-next/ffmpeg-next, accepting maintenance-mode risk, or a thin in-house FFI) or GStreamer. Composite in wgpu. Present via `RenderImage` everywhere and `surface()` on macOS.
3. Speech: official `sherpa-onnx` crate as the single engine for STT (Parakeet/Whisper/Moonshine), VAD (Silero/TEN), TTS (Kokoro/Kitten/Matcha) and in-process cloning (Pocket TTS, ZipVoice). Keep `ort` 2.0-rc for custom ONNX models. Consider `transcribe-cpp`/whisper-rs for GGML/Metal paths.
4. Audio: cpal 0.18 (now covers PipeWire and loopback on all 3 OSes) + rubato + symphonia; `screencapturekit` for macOS app-audio capture.
5. Dictation: `handy-keys` for hotkeys (works on Wayland via evdev), `arboard` with `wayland-data-control`, `enigo` + external wtype/dotool/ydotool fallbacks on Wayland, and ashpd portals as the long-term path.
6. Extensibility: cargo features for first-party modules; wasmtime component model (Zed's approach) for third-party plugins; sidecar processes for Python and GPL tools. Avoid `abi_stable` (dormant); use `stabby` only if native dylib plugins are unavoidable.

## Unverified / gaps
- Discord shipping GPUI; whisper-rs last commit (codeberg blocked); pipewire-rs last commit (GitLab blocked); Kokoros license (the repository reports Apache-2.0, checked 2026-11-13); Pocket TTS model license; which sherpa-onnx TTS builds link espeak-ng; per-DE support for the GlobalShortcuts and RemoteDesktop portals; Python-only cloning models; other Rust NLE projects.
