# Crastu roadmap

Last updated 2026-11-11. This page is the plan of record; [ARCHITECTURE.md](ARCHITECTURE.md) explains the crates it mentions and [DECISIONS.md](DECISIONS.md) records why.

Progress lives in the GitHub [milestones](https://github.com/ajianaz/crastu/milestones). Each phase below has one milestone.

## How the plan works

- **One phase at a time.** A phase ends when its exit criteria are met, not on a date. Dates are deliberately absent: Crastu is personal-first and free, and the order matters more than the calendar.
- **Design before code** (decision 13). Every phase starts with its screens in the design canvas, reviewed against [DESIGN.md](DESIGN.md), before UI work begins.
- **Spikes before bets.** Anything with an unknown that could change the architecture gets a time-boxed spike with a written result in `docs/spikes/`.
- **Every module ships with its MCP tools** (decision 4). A feature is not done until an external agent can use it under the approval policy.
- **Licence gate on every model and dependency** (decision 2). New entries in the model catalogue carry a licence id and a commercial flag; `cargo deny` guards code dependencies.
- **Releases start at P1.** P0 produces no release. Versions stay below 1.0 until the v1.0 criteria hold.

## Overview

| Phase | Outcome | Release | Platform | Status |
|---|---|---|---|---|
| [P0 Foundation](#p0-foundation) | Workspace, core, host and MCP, design system in GPUI, shell | none | macOS (CI: Linux, macOS) | In progress |
| [P1 Dictation](#p1-dictation-macos--v01) | Hold a key, talk, text lands in any app | v0.1 | macOS | Next |
| [P2 Voice](#p2-voice--v02) | Script to voiceover with presets and user engines | v0.2 | macOS | Planned |
| [P3 Auto-clipper](#p3-auto-clipper--v03) | Long video to short captioned clips | v0.3 | macOS | Planned |
| [P4 Editor](#p4-editor--v04) | Timeline editing, render and export | v0.4 | macOS | Planned |
| [P5 Windows](#p5-windows--v05) | Everything above on Windows | v0.5 | macOS, Windows | Planned |
| [v1.0](#v10) | Stable formats and tools, audited, documented | v1.0 | macOS, Windows | Planned |
| [Later](#later) | Dub, meetings, plugins, Linux, mobile, sync, cloning | — | — | Ideas |

```
P0 ──► P1 ──► P2 ──► P3 ──► P4 ──► P5 ──► v1.0
        │             ▲
        └─ memory v1 ─┘ (memory grows with each phase)
```

## P0 Foundation

**Goal:** the skeleton every feature plugs into, proven on macOS, with the risky unknowns answered.

| Item | Crates or files | Issue | Status |
|---|---|---|---|
| Workspace, layering checks (`deps-check`, `build-matrix`, `mobile-check`), `cargo deny` | `xtask`, `deny.toml` | — | Done |
| Core types, module registry, store, settings and secrets, jobs, model catalogue | `crastu-core`, `-module`, `-store`, `-settings`, `-jobs`, `-models` | — | Done |
| UI-neutral view models (appearance, navigation, onboarding) | `crastu-app` | — | Done |
| GitHub as primary, CI on Linux and macOS, Cora review | `.github/` | #11 | Done |
| Contributing guide, security policy, issue forms, Dependabot | repo root | #12 | Done |
| Headless host, local IPC, stdio MCP shim, approval policy, activity log | `crastu-host`, `-ipc`, `-mcp`, `crastu-cli` | #5 (#14) | Done |
| Spike: one ONNX Runtime for sherpa-onnx and uteke | `docs/spikes/onnx-runtime.md` | #2 | Open |
| Spike: decoded video frames in GPUI on macOS | `docs/spikes/gpui-video.md` | #3 | Done: VideoToolbox into `surface()`; colour and 60 fps follow-ups |
| Spike: push-to-talk hotkey and paste on macOS | `docs/spikes/macos-input.md` | #4 | Open (needs a Mac) |
| Design tokens and components in GPUI, gallery window | `crastu-ui` | #6 | Open |
| Window, rail, sidebars, inspector, Cmd+K, settings, onboarding | `crastu-shell`, `crastu-desktop` | #7 | Open |

**Exit criteria**

1. `crastu-desktop` opens on macOS with rail, left sidebar, content and inspector; theme (Graphite, Paper, Signal), font pairing and density switch live.
2. Onboarding runs end to end with real macOS permission checks.
3. `claude mcp add --transport stdio crastu -- crastu mcp` lists Crastu's tools in Claude Code, and a Change-tier call respects Ask, Auto and Deny.
4. All three spikes have a written result and, where they change the plan, a decision in `DECISIONS.md`.
5. CI is green on Linux and macOS, including the iOS and Android compile check.

## P1 Dictation (macOS) — v0.1

**Goal:** the first useful release. Hold a key, talk, and clean text lands in the focused app, fully on device unless the user adds a provider.

| Area | Scope | Crates |
|---|---|---|
| Audio | Mic capture, resampling, Silero VAD, warm mic with pre-roll, level meter | `crastu-audio` |
| Speech to text | `Transcriber` seam; sherpa-onnx adapter (Whisper Small, Large v3 Turbo; Parakeet where licence allows); streaming partials; cloud adapter (BYOK) | `crastu-stt` |
| Input | Hold-to-talk and toggle hotkeys (modifier-only and Fn), focus capture, paste with clipboard restore, typing fallback | `crastu-os-input` |
| Permissions | Microphone, Accessibility, Input Monitoring status and guides | `crastu-os-perms` |
| Cleanup | `TextModel` seam with Anthropic and OpenAI-compatible adapters (BYOK, keys in Keychain); rules fallback without a key; per-app styles; custom vocabulary | `crastu-llm`, `crastu-dictation` |
| Models | Catalogue UI: download with progress and resume, sha256 check, licence badge, delete | `crastu-models`, `crastu-shell` |
| Dictation UI | Floating pill with partials, history list with search, per-app settings | `crastu-dictation-ui` |
| Memory v1 | Embedded uteke with EmbeddingGemma 300M; dictation history and preferences indexed; Cmd+K searches them | `crastu-memory` |
| MCP | `dictation_history_search`, `transcribe_file`, `moments_search` | module tools |
| Packaging | Signed and notarised `.dmg`, update check (no auto-install yet), crash logs kept local | `xtask`, CI |

**Exit criteria**

1. Works in VS Code, Chrome, Slack, Terminal, Notes and Mail without losing the clipboard.
2. Target latency: under 1 s from key-up to text for a 10-second utterance with Whisper Small on an M1 (measured and recorded; a miss is a decision, not a silent slip).
3. Works offline after the first model download; no network call without a provider configured.
4. A fresh install goes from download to first dictation in under 5 minutes, guided by onboarding.
5. Release notes, a README quick start and a known-issues list ship with v0.1.

## P2 Voice — v0.2

**Goal:** turn a script into a voiceover with built-in presets or an engine the user brings.

**Before starting:** settle open question 3 (Supertonic upstream is archived: confirm the weights can be pinned and redistributed).

| Area | Scope | Crates |
|---|---|---|
| TTS seam | `Synthesizer` with capabilities (presets, languages, streaming, cloning) | `crastu-tts` |
| Built-in | Supertonic 3, 10 presets, through sherpa-onnx; labelled Conditional (OpenRAIL-M) | `crastu-tts`, `crastu-models` |
| User engines | Local server (OpenAI speech API or Crastu engine protocol), cloud BYOK, command engine; user declares the licence, the UI shows it (decision 7) | `crastu-tts`, `crastu-sidecar` |
| Voice studio | Script editor, per-paragraph voice and speed, takes with compare and pick, regenerate one line | `crastu-voice`, `crastu-voice-ui` |
| Export | WAV and MP3, loudness normalisation, SRT from the script timing | `crastu-voice` |
| Memory | Remembers preferred voices and settings per project | `crastu-memory` |
| MCP | `tts_generate`, `voice_list`, `voice_take_pick` | module tools |

**Exit criteria**

1. A 5-minute script renders with a preset, and a user-added local engine works through the same screen.
2. Every voice shows its licence; non-commercial engines are never offered for download by Crastu.
3. An agent can generate and pick a take through MCP under the approval policy.

## P3 Auto-clipper — v0.3

**Goal:** from a long video, suggest the best moments and export them as reframed, captioned short clips.

**Before starting:** spike #3 (GPUI video) done; word timestamps confirmed (whisper.cpp DTW or a transducer model).

| Area | Scope | Crates |
|---|---|---|
| Media | FFmpeg 8 LGPL shared builds pinned by sha256: probe, decode, encode, waveforms, thumbnails | `crastu-media` |
| Vision | YuNet face detection, scene cuts, tracking, smoothed crop path, hard cuts at scenes | `crastu-vision` |
| Signals | Transcript with word timestamps, vocal energy, pauses, laughter | `crastu-clipper` |
| Scoring | LLM rubric when a provider is set, heuristics otherwise; learns from accept and reject through memory | `crastu-clipper`, `crastu-memory` |
| Captions | Caption model, styles, word-by-word animation, safe areas | `crastu-captions` |
| Output | 9:16, 1:1 and 16:9 presets, loudness at −14 LUFS, each clip saved as an editable timeline project | `crastu-timeline` (minimal), `crastu-render` (minimal) |
| Clipper UI | Candidate list with scores and reasons, preview, accept or reject, batch export | `crastu-clipper-ui` |
| MCP | `clip_suggest`, `clip_list`, `clip_accept`, `clip_reject`, `clip_export`, `asset_import` | module tools |

**Exit criteria**

1. A 60-minute talking-head video yields ranked candidates; a 1-minute clip exports with reframing and captions.
2. Without any provider, heuristics still produce usable suggestions.
3. Exported clips reopen as timeline projects (ready for P4).

## P4 Editor — v0.4

**Goal:** a timeline editor for clips, voice, music and captions, also drivable by agents.

| Area | Scope | Crates |
|---|---|---|
| Timeline | Tracks for video, voice, music and captions; trim, split, move, ripple, snapping; undo and redo; autosave | `crastu-timeline` |
| Render | Pure `render_at(t)`, content-hash frame cache, preview at reduced resolution, music ducking under voice | `crastu-render` |
| Export | Presets per platform, background export through jobs, progress and cancel | `crastu-render`, `crastu-jobs` |
| Integration | Voiceovers from P2 and clips from P3 drop straight onto the timeline | `crastu-editor-ui` |
| Editor UI | Timeline, viewer, media bin, inspector for the selected item | `crastu-editor-ui` |
| MCP | `timeline_get` (resource), `timeline_apply_edits` (batch with dry-run diff), `export_render` | module tools |

**Exit criteria**

1. A 10-minute project with four tracks plays back smoothly in preview on an M1 and exports correctly.
2. An agent's edit batch shows a diff, needs approval when policy says Ask, and is undoable in one step.
3. Project files reopen after an app upgrade (migrations tested).

## P5 Windows — v0.5

**Goal:** everything from P1 to P4 on Windows 11.

| Area | Scope |
|---|---|
| Input | Low-level keyboard hook, `SendInput` paste and typing, focus capture |
| Permissions | Microphone privacy settings, guidance where Windows needs it |
| Audio | WASAPI capture and loopback |
| Video | CPU frame copy for preview in v0.5; texture path later (open question 2) |
| Secrets | Windows Credential Manager |
| Packaging | Signed installer, update check |
| CI | Windows runner added to the required checks |

**Exit criteria:** the P1 to P4 exit criteria pass on Windows 11, and CI runs the full test suite on Windows.

## v1.0

Crastu reaches 1.0 when all of these hold:

1. Project and settings formats are stable, versioned and migrated automatically.
2. The MCP tool set is versioned; breaking changes go through a deprecation period.
3. A licence audit covers every shipped dependency and model, published in the repository.
4. An accessibility pass is done: keyboard-only use, screen reader labels, contrast in all three themes.
5. User documentation covers every feature, including setting up agents.
6. No known data-loss bugs; crash logs stay local and are shared only by the user.

## Later

Not scheduled. Each idea gets a design and a decision before it moves into a phase.

| Idea | Note |
|---|---|
| Dub and translation | Transcript, translate with BYOK, re-voice with P2 engines, lip timing |
| Meeting capture | System audio per app (`screencapturekit` on macOS), notes into memory |
| On-device voice cloning | Only with a model whose weights allow commercial use (open question 4) |
| Plugins | WebAssembly components (wasmtime, versioned WIT) for light plugins; command engines for heavy ones |
| Linux | After Windows; Wayland input and paste need their own spike |
| Mobile | New front end over `crastu-app` through flutter_rust_bridge (or UniFFI); host runs in process (decision 22) |
| Optional sync | `crastu-sync` to a hosted memory service, off by default |
| MCP over HTTP | Local Streamable HTTP on 127.0.0.1 with bearer token and Origin check, next to stdio |
| Web | Last of all platforms (decision 26): a remote front end over the host and `crastu-app` state, not a WebAssembly port of the GPUI UI. Dictation stays desktop-only |

## Work that runs through every phase

| Track | In every phase |
|---|---|
| Design | Screens for the phase in the design canvas first; components added to `crastu-ui`, never styled ad hoc |
| MCP | Each new module registers its tools, with risk tiers and tests through the stdio shim |
| Memory | Each module decides what it remembers and how agents can query it |
| Licences | Model catalogue entries with licence and commercial flag; `cargo deny` on every pull request |
| Mobile readiness | Domain and app crates keep compiling for iOS and Android (`cargo xtask mobile-check`) |
| Quality | Unit tests in each crate, an end-to-end test per MCP tool, CI green before merge |
| Docs | `ARCHITECTURE.md` and `DECISIONS.md` updated in the same pull request as the change |

## Risks

| Risk | Impact | Mitigation |
|---|---|---|
| Two ONNX Runtimes in one process (sherpa-onnx and uteke) | Crashes or doubled memory | Spike #2; fallbacks: embedder out of process, or provider embeddings |
| GPUI is pre-1.0 and changes often | Breaking upgrades | Pin `gpui-component` and `gpui-pre` together, upgrade on purpose, keep UI thin over `crastu-app` |
| Supertonic upstream archived | Cannot update or redistribute | Confirm licence before P2; the `Synthesizer` seam allows a replacement |
| FFmpeg packaging and LGPL duties | Licence breach or broken builds | Shared builds only, pinned by sha256, licence notice in the app |
| macOS permissions (TCC) confuse users | Dictation looks broken | Onboarding checks each permission live and explains how to grant it |
| Windows video preview performance | Choppy editor on Windows | CPU copy for v0.5, texture path researched in parallel |
| No commercially clean cloning model | Feature gap against other apps | Keep cloning user-installed with licence badges (decision 7) |
