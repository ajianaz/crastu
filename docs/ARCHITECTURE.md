# Crastu architecture, draft v2

Status: **draft, not yet approved for implementation** (2026-11-10). Replaces the earlier analyses made under the working names Gema and Cordio.

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

## 3. Workspace

```
crastu/
├─ apps/
│  ├─ crastu-desktop/        GPUI app, composition root; Cargo features pick modules
│  └─ crastu-cli/            headless CLI; `crastu mcp` is the stdio MCP shim
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
│  └─ ui/
│     ├─ crastu-ui/          design tokens and components on gpui-component
│     └─ crastu-shell/       window, rail, sidebars, command palette, onboarding
└─ xtask/                    dependency-graph lint, per-feature build matrix, bundling, licence checks
```

### Dependency rules (checked in CI)

```
apps → features/*-ui → ui/* → gpui-component
apps → features/* (domain) → engines/*, integrations/*, os/* → core/*
```

- `features/<a>` must not depend on `features/<b>`, except crates marked composite (`crastu-clipper` uses `crastu-timeline`).
- Domain crates must not depend on `gpui`.
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
| UI ↔ domain | GPUI, CLI, MCP | One registry, three adapters |

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
- Library: `rmcp` 3.9 (Apache-2.0), spec 2026-07-28 with backward compatibility.

## 6. Data flows

**Dictation**: key down → capture focus target and app category → mic (warm, pre-roll) → streaming STT with partials in the pill → key up → optional cleanup through the routed `TextModel` (timeout falls back to raw) → insert (paste and restore, or type) → history.

**Auto-clipper**: transcript with word timestamps → signals (scene cuts, vocal energy, pauses, laughter) → candidate windows → scoring (LLM rubric if a provider is set, heuristics otherwise; preferences from memory) → reframe (face detection, active speaker, smoothed crop path, hard cuts at scenes) → captions → loudness −14 LUFS → an editable timeline project.

**Memory**: segment summaries, preferences and project context are written through `crastu-memory` into a local uteke database; read by ⌘K search, the clipper, and agents through MCP.

## 7. Technology choices (verified 2026-11-10)

| Need | Choice |
|---|---|
| UI | `gpui-component` 0.9.1 (GPUI Kit, Apache-2.0) pinned with `gpui-pre` 0.3.12; upgrade weekly in lockstep |
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

## 9. Reuse from the CodeCora ecosystem

| Repo | Use |
|---|---|
| uteke (Apache-2.0) | Library: `crastu-memory` |
| uteke-cloud (proprietary) | HTTP API only: `crastu-sync` |
| tole (MIT) | External MCP client of Crastu; patterns (risk tiers, approver, rmcp registry) |
| vetio | Concepts only: pure `render_at(t)`, content-hash render cache, SRT/VTT parsing, music ducking, safe areas |

## 10. Roadmap (proposed)

| Phase | Content |
|---|---|
| P0 Foundation | Workspace, xtask checks, core crates, host and IPC, MCP skeleton, `crastu-ui` tokens and components in GPUI, shell, onboarding |
| P1 Dictation (macOS) | Audio, STT, input, cleanup with BYOK or rules, history, pill |
| P2 Voice | TTS seam, Supertonic, engine adapters, takes |
| P3 Auto-clipper | Media, vision, captions, scoring, minimal timeline output |
| P4 Editor | Full timeline, render, export |
| P5 Windows | Input, permissions, packaging |
| Later | Dub, meeting capture, WASM plugins, Linux |

## 11. Open questions

1. ONNX Runtime sharing between sherpa-onnx and uteke's `ort` (spike in P0).
2. GPUI video frames on Windows: CPU copy for v1, texture path later.
3. Supertonic upstream is archived; confirm we can pin and redistribute the weights.
4. A commercially clean on-device cloning model (Pocket TTS, ZipVoice, Chatterbox) to verify.
5. Repository location and visibility (CodeCora Gitea, private or public).
