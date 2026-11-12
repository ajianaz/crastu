# Crastu architecture, v2

Status: **approved for P0** (2026-11-10), including the UI-neutral app layer for future mobile clients. Replaces the earlier analyses made under the working names Gema and Cordio.

## 1. Product scope

Crastu is a free, personal-first, desktop app (macOS first, then Windows; Linux later) that combines:

- **Dictation**: hold a key, talk, clean text lands in any app. Concepts from OpenWhispr.
- **Voice**: text to speech with preset voices and user-added engines. Concepts from VoiceStudio.
- **Auto-clipper**: long video to short reframed clips with captions.
- **Editor**: a timeline editor. Concepts from OpenCut.
- **Memory and agents**: an embedded uteke memory and a built-in MCP server so external AI agents can operate the app.

References are used for **concepts only**; no code is copied from VoiceStudio (AGPL), OpenWhispr or OpenCut.

## 2. Hard rules

1. Licence: **Apache-2.0**.
2. Everything Crastu ships or downloads must allow **commercial use**, code and model weights alike. Conditional licences (OpenRAIL-M, Gemma Terms) are allowed and shown as "Conditional". User-installed engines carry the licence the user declares.
3. **No bundled LLM.** Language models are bring-your-own-key. Without a key, features fall back to rules and heuristics.
4. **No telemetry.** Network calls only for model downloads, update checks and providers the user adds.
5. Feature modules never import each other (composite modules excepted). Domain crates never depend on GPUI.
6. Every action by an external agent passes the approval policy and is logged; changes are undoable and deletions go to the trash.
7. UI and logic are separate. Screen state and actions live in `crastu-app`; every UI (GPUI today, Flutter or native mobile later) only renders that state and sends actions.
8. Core, app and domain crates compile for `aarch64-apple-ios` and `aarch64-linux-android`. OS-specific code sits behind traits with per-platform adapters.

## 3. Workspace

```
crastu/
├─ apps/
│  ├─ crastu-desktop/        GPUI app, composition root; Cargo features pick modules
│  └─ crastu-cli/            headless CLI as a library and a binary; the desktop `crastu` binary answers `crastu mcp` (the stdio MCP shim), `host`, `stop` and `version` without a window
├─ crates/
│  ├─ core/
│  │  ├─ crastu-core/        shared types: ids, timecode, audio buffers, asset refs, errors
│  │  ├─ crastu-module/      Module trait, registry, intents, capability queries
│  │  ├─ crastu-host/        headless core process: modules, jobs, store, engines
│  │  ├─ crastu-ipc/         local socket protocol between host, GUI and CLI shim
│  │  ├─ crastu-store/       SQLite (rusqlite 0.41 bundled, FTS5), per-module migrations
│  │  ├─ crastu-settings/    one settings source; secrets in the macOS Keychain / Windows Credential Manager
│  │  ├─ crastu-jobs/        queue, GPU/CPU gate, progress, cancel, persistence
│  │  └─ crastu-models/      catalogue, resumable download, sha256 pins, licence metadata
│  ├─ engines/
│  │  ├─ crastu-audio/       capture, playback, resample, VAD, system audio (feature)
│  │  ├─ crastu-stt/         trait Transcriber; adapters: sherpa-onnx, whisper.cpp, cloud (BYOK)
│  │  ├─ crastu-tts/         trait Synthesizer; adapters: built-in, local server, cloud, command
│  │  ├─ crastu-llm/         trait TextModel; adapters: Anthropic, OpenAI-compatible (OpenAI, OpenRouter, Groq, Gemini, custom)
│  │  ├─ crastu-media/       FFmpeg (LGPL, dynamically linked): probe, decode, encode, waveforms, thumbnails
│  │  ├─ crastu-vision/      face detection (YuNet), scene cuts, tracking, crop-path smoothing
│  │  ├─ crastu-captions/    caption model, styles, word animation, renderer
│  │  └─ crastu-sidecar/     process supervisor and length-prefixed JSON protocol for command engines
│  ├─ integrations/
│  │  ├─ crastu-mcp/         rmcp router; tools contributed by modules; stdio and optional local HTTP
│  │  ├─ crastu-memory/      uteke-core adapter (embedded, local)
│  │  └─ crastu-sync/        optional Uteke Cloud HTTP client (off by default)
│  ├─ os/
│  │  ├─ crastu-os-input/    global hotkeys, push-to-talk, text insertion, focus target
│  │  └─ crastu-os-perms/    permission status and guides (TCC on macOS)
│  ├─ features/
│  │  ├─ dictation/          crastu-dictation, crastu-dictation-ui
│  │  ├─ voice/              crastu-voice, crastu-voice-ui
│  │  ├─ clipper/            crastu-clipper (composite), crastu-clipper-ui
│  │  └─ editor/             crastu-timeline, crastu-render, crastu-editor-ui
│  ├─ app/
│  │  ├─ crastu-app/         UI-neutral view models: screen state, actions, event stream (no GPUI)
│  │  └─ crastu-ffi/         later: thin flutter_rust_bridge adapter over crastu-app (UniFFI for native apps)
│  └─ ui/
│     ├─ crastu-ui/          design tokens and components on gpui-component
│     └─ crastu-shell/       window, rail, sidebars, command palette, onboarding; renders crastu-app state
└─ xtask/                    dependency-graph lint, per-feature build matrix, bundling, licence checks
```

### Dependency rules (checked in CI)

```
apps → features/*-ui → ui/* → gpui-component
features/*-ui → crastu-app
crastu-ffi (mobile, later) → crastu-app
crastu-app → features/* (domain) → engines/*, integrations/*, os/* → core/*
```

- `features/<a>` must not depend on `features/<b>`, except crates marked composite (`crastu-clipper` uses `crastu-timeline`).
- Domain crates and `crastu-app` must not depend on `gpui`.
- `*-ui` crates hold no business state: they render `crastu-app` state and dispatch its actions.
- CI compile-checks core, app and domain crates for iOS and Android targets.
- CI builds each feature alone (`--no-default-features --features dictation`, and so on). That is the working definition of "detachable".

### Detaching modules, three layers

1. **Build time**: Cargo features in `crastu-desktop`.
2. **Runtime**: modules can be disabled in Settings; models and engines load lazily.
3. **Third party (later)**: WebAssembly components (wasmtime, versioned WIT) for light plugins; command engines for heavy ones.

## 4. Key seams

| Seam | Adapters (at least two) | Notes |
|---|---|---|
| `Transcriber` | sherpa-onnx (Whisper, Parakeet), whisper.cpp, cloud BYOK | Word timestamps needed by the clipper: whisper.cpp DTW or a transducer model |
| `Synthesizer` | built-in Supertonic 3 (sherpa-onnx), local server (OpenAI speech API or Crastu engine protocol), cloud BYOK, command | Capabilities: presets, cloning, languages, streaming |
| `TextModel` | Anthropic, OpenAI-compatible | Per-task routing: dictation cleanup, clip scoring, translation, voice commands |
| `TextSink` / `HotkeySource` | macOS (CGEvent, AX, pasteboard), Windows (SendInput, low-level hook) | Paste with clipboard restore after the target reads; typing fallback |
| `Embedder` (inside crastu-memory) | EmbeddingGemma 300M local (default), provider embeddings (optional) | Default keeps everything on the Mac |
| UI ↔ app state (`crastu-app`) | GPUI, CLI, MCP; later flutter_rust_bridge or UniFFI | One registry and one set of view models, many front ends |

## 5. MCP

```
Claude Code / Claude Desktop / tole
   │ stdio (default)                    │ Streamable HTTP (optional, 127.0.0.1, bearer token, Origin check)
   ▼                                    ▼
crastu mcp (shim) ── local IPC ──► crastu-host ◄── GUI (in process)
```

- `crastu mcp` starts `crastu --headless` if no host is running, so agents work with the window closed.
- Each module registers its tools. Draft set: `project_*`, `asset_import`, `job_status/cancel`, `transcribe_file`, `dictation_history_search`, `tts_generate`, `voice_list`, `clip_suggest/list/accept/reject/export`, `timeline_get` (resource), `timeline_apply_edits` (batch with dry-run diff), `export_render`, `moments_search`.
- Risk tiers: Read, Change, Delete. Policy per tier: Read is always Auto; Change is Ask or Auto; Delete is Ask, Auto or Deny. Policy can be set per agent. Pattern adopted from tole (MIT).
- Agent pairing (decision 25): the name a client declares (`--agent`) is only a label. `crastu agents add NAME` creates a random token (`crastu_` plus 64 hex digits, shown once; the host stores its SHA-256 hash in the `agents` table). The shim reads `CRASTU_AGENT_TOKEN` and sends it with every `tools.call`; the host resolves token to agent (constant-time compare, looked up on every call) and applies that agent's policy override. Callers without a token are unpaired: defaults only, marked unpaired in the activity log. A token that is unknown, rotated or revoked is refused with an error. `crastu agents list|revoke|rotate` manage the pairings from the data folder (`CRASTU_DATA`). Claude Code: `claude mcp add --transport stdio --env CRASTU_AGENT_TOKEN=<token> crastu -- crastu mcp` (this puts the token in shell history and, briefly, in the process list; the preferred way is the `env` block of the client's configuration file). A per-agent override held under a name also restricts an unpaired caller that declares that name (the stricter of override and default applies), so a Deny cannot be dodged by leaving the token out; an override never grants an unpaired caller more than the default.
- Library: `rmcp` 3.9 (Apache-2.0), spec 2026-07-28 with backward compatibility.

## 6. Data flows

**Dictation**: key down → capture focus target and app category → mic (warm, pre-roll) → streaming STT with partials in the pill → key up → optional cleanup through the routed `TextModel` (timeout falls back to raw) → insert (paste and restore, or type) → history.

**Auto-clipper**: transcript with word timestamps → signals (scene cuts, vocal energy, pauses, laughter) → candidate windows → scoring (LLM rubric if a provider is set, heuristics otherwise; preferences from memory) → reframe (face detection, active speaker, smoothed crop path, hard cuts at scenes) → captions → loudness −14 LUFS → an editable timeline project.

**Memory**: segment summaries, preferences and project context are written through `crastu-memory` into a local uteke database; read by ⌘K search, the clipper, and agents through MCP.

## 6b. Mobile readiness

Mobile is not in scope yet, but the code is shaped so it can be added by writing a new front end, not by moving logic:

- `crastu-app` exposes plain data (state snapshots), actions and an event stream. GPUI renders it on desktop; a Flutter app would render the same state through `crastu-ffi` (flutter_rust_bridge), or a native app through UniFFI.
- On desktop the host is a separate headless process; on mobile it runs in process inside the app.
- Platform adapters needed on mobile: audio capture, permissions, text insertion (an iOS keyboard extension instead of global paste), media decoding (platform codecs instead of FFmpeg).
- Some features will be reduced on phones (for example the full timeline editor); their mobile screens are designed separately when the time comes.

## 7. Technology choices (verified 2026-11-10)

| Need | Choice |
|---|---|
| UI | `gpui-component` 0.9.1 (GPUI Kit, Apache-2.0) pinned with `gpui-pre` 0.3.12, exact versions in the workspace manifest; upgrade weekly in lockstep. GPUI is a dependency on macOS and Windows only for now (Linux follows Windows), so the crates are empty on Linux and the Linux CI job needs no X11 or Wayland libraries. Metal shaders are compiled at launch (`runtime_shaders`) until the release build decision (issue #41). `deny.toml` lists the unmaintained-crate notices that come with GPUI, each with a reason and a review date |
| On-device speech | `sherpa-onnx` 2.0.1 official crate (Apache-2.0); `whisper-rs` 0.16 optional |
| ONNX | `ort` 2.0.0-rc.15, `load-dynamic`; one shared ONNX Runtime with uteke (spike required) |
| Audio | `cpal` 0.18, `rubato`, `symphonia`; `screencapturekit` for per-app audio on macOS |
| Video | FFmpeg 8.x LGPL shared builds, pinned by sha256 (pattern from OpenCut) |
| Input | `global-hotkey`, macOS CGEvent/AX via objc2, Windows `SendInput`; `arboard` |
| Storage | `rusqlite` 0.41 bundled with FTS5; `keyring` |
| MCP | `rmcp` 3.9 |
| Memory | `uteke-core` 0.23 (Apache-2.0), `vecq` backend |

## 8. Models shipped or offered

| Model | Task | Licence | Status |
|---|---|---|---|
| Whisper Small / Large v3 Turbo | STT | MIT | Allowed |
| Silero VAD | VAD | MIT | Allowed |
| Supertonic 3 | TTS, 10 presets | OpenRAIL-M | Conditional |
| YuNet | Face detection | MIT | Allowed |
| EmbeddingGemma 300M | Memory search | Gemma Terms of Use | Conditional, default |
| OmniVoice, others | Cloning | Non-commercial | User-installed only, never shipped |

Denied for shipping: anything NC, gated or GPL linked into the main process (OmniVoice weights, pyannote, SCRFD/InsightFace, espeak-ng, GSAP, Edge TTS).

### Catalogue and downloader

`crastu-models` embeds the catalogue (`catalogue.json`): per model the licence id and URL, the commercial flag (`Allowed`, or `Conditional` with a note), the exact download URL, and the sha256 and size measured on the real file. `catalogue()` validates it and a `Denied` entry does not load. Today it lists Whisper Small (the sherpa-onnx archive, 639 MB, int8 and fp32 builds in one tar.bz2; unpacking is a later slice), Silero VAD and YuNet; Supertonic 3 stays out until open question 3 is answered. `crastu-shell` reads names, uses, sizes and licence badges from it.

`download_with` resumes a `<file_name>.part` through a `Fetch` trait (a ranged GET that says where the body starts and the total size), reports progress, honours a cancel flag, checks size and sha256, and only then renames to the final name; a wrong checksum or too many bytes deletes the part, a cut connection or a cancel keeps it, an operating-system lock on `<file_name>.part.lock` (released by the system when the process dies, so a killed download resumes on the next run) keeps two downloads of one model apart, and a server that ignores `Range` or a file that changed on the server restarts from byte 0. The real `Fetch` is `crastu-models-http` (`ureq` with rustls, https only); the network is touched only when a download is called. Human check: `crastu models list` and `crastu models download <id> [--dir PATH]` (also `crastu-cli`).

### Offline mode

`crastu-core::net` holds the one gate for outbound requests (rule 4 above). `NetGate` is cheap to clone and shares one switch; a caller asks `gate.check("model download whisper-small", url)` before opening a request and gets `Err(Offline { what, host })` when Offline is on, for example `Offline: model download whisper-small from huggingface.co refused`. Only the host appears in the message. The gate keeps the last 100 refusals (what, host, a sequence number) for `refusals()` and `drain_refusals()`, which the activity log will read. The switch is the setting `network.offline` (off by default) and the environment variable `CRASTU_OFFLINE`; `crastu-settings::offline_enabled` resolves them, with a non-empty environment value winning over the setting (an unrecognised value counts as on; empty or unset leaves the setting in charge). Without a data folder and without the variable, the CLI says it cannot locate the setting rather than assuming online. In `crastu-models`, `GatedFetch` wraps any `Fetch` and refuses before the inner adapter runs, so no socket is opened; a model that is already on disk and verified is used without any request, and `require_installed` says how to get a missing one. `crastu models download` and `crastu models list` build the gate from `settings.json` in the data folder and `CRASTU_OFFLINE`; the desktop app and the CLI share one lookup for that folder (`crastu_settings::data_dir`: `CRASTU_DATA`, else `~/Library/Application Support/Crastu` on macOS, `%APPDATA%\Crastu` on Windows, `$XDG_DATA_HOME/crastu` or `~/.local/share/crastu` elsewhere), so the setting is the same in both. Every new request type (update check, providers, sync) must go through the same gate.

## 9. Reuse from the CodeCora ecosystem

| Repo | Use |
|---|---|
| uteke (Apache-2.0) | Library: `crastu-memory` |
| Uteke Cloud (hosted service) | HTTP API only: `crastu-sync` |
| tole (MIT) | External MCP client of Crastu; patterns (risk tiers, approver, rmcp registry) |
| vetio | Concepts only: pure `render_at(t)`, content-hash render cache, SRT/VTT parsing, music ducking, safe areas |

## 10. Roadmap

The phased plan, with scope, exit criteria and risks, is in [ROADMAP.md](ROADMAP.md): P0 Foundation, P1 Dictation (v0.1), P2 Voice (v0.2), P3 Auto-clipper (v0.3), P4 Editor (v0.4), P5 Windows (v0.5), then v1.0.

## 11. Open questions

1. ONNX Runtime sharing between sherpa-onnx and uteke's `ort` (spike in P0).
2. GPUI video frames on Windows: CPU copy for v1, texture path later.
3. Supertonic upstream is archived; confirm we can pin and redistribute the weights.
4. A commercially clean on-device cloning model (Pocket TTS, ZipVoice, Chatterbox) to verify.
5. Resolved: the repository is public on GitHub (`ajianaz/crastu`); Gitea keeps a pull mirror as backup.
