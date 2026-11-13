# Media pipeline: backends, audio I/O and codec licences

Research verified on 2026-11-10 (crate data from `https://crates.io/api/v1/crates/<name>`; READMEs and source via raw.githubusercontent.com). Access limits: GitHub REST API, docs.rs and gitlab.freedesktop.org were blocked by the proxy, so a few last-commit dates are missing.

Related notes: [auto-clipper.md](auto-clipper.md) (export checks), [speech.md](speech.md), [rust-ecosystem.md](rust-ecosystem.md) (GPUI video frames), [../spikes/gpui-video.md](../spikes/gpui-video.md).

Not researched yet: proxy generation and the multi-track mixer. Add them here when they are. Researched since 2026-11-13: the native macOS backends against FFmpeg, pure-Rust MP4 and Symphonia, what a native path cannot read, Media Foundation and a licence and patent table per backend, in [../spikes/media-backends.md](../spikes/media-backends.md) (measured on one M2; Windows is sources only). Still not researched: GStreamer as a backend, Linux hardware decode (VA-API), fragmented MP4 and HLS output.

## Prior art

Every project consulted, on equal footing. "Concepts only" marks a project whose licence does not allow us to reuse its code. Licences were read from each GitHub repository on 2026-11-13 unless a version date is given in the tables below.

| Project | Licence | Concept taken | Link |
|---|---|---|---|
| OpenCut (OpenCut-app/OpenCut) | MIT; Crastu takes concepts only (decision 1) | Rust core with a shared LGPL FFmpeg: pinned build, licence file and configure flags shipped with it | https://github.com/OpenCut-app/OpenCut |
| FFmpeg (via ffmpeg-next, ffmpeg-sys-next, rsmpeg) | LGPL-2.1-or-later when built without `--enable-gpl` and `--enable-nonfree` | Dynamic linking of an LGPL build; platform encoders for H.264/HEVC | https://ffmpeg.org |
| GStreamer | LGPL (core); codec plugins vary | Alternative decode path through the official Rust bindings | https://gstreamer.freedesktop.org |
| wgpu | MIT OR Apache-2.0 | GPU compositing on our own device | https://github.com/gfx-rs/wgpu |
| cpal, rodio, rubato, symphonia | Apache-2.0, MIT OR Apache-2.0, MIT OR Apache-2.0, MPL-2.0 | Audio capture, playback, resampling and decode in Rust | https://crates.io/crates/cpal |
| screencapturekit (doom-fish) | MIT OR Apache-2.0 | macOS system audio and screen capture | https://github.com/doom-fish/screencapturekit-rs |

## Audio I/O

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


## Video

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

