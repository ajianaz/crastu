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
8. Core, app and domain crates compile for `aarch64-apple-ios` and `aarch64-linux-android`, and for the 32-bit `armv7-linux-androideabi` and `i686-linux-android` (which catch code that assumes 64-bit C types). OS-specific code sits behind traits with per-platform adapters.

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
│  │  ├─ crastu-models/      catalogue, resumable download, sha256 pins, licence metadata
│  │  ├─ crastu-recorder/    flight recorder: a local session folder per run (events, perf, panic)
│  │  ├─ crastu-logfiles/    session folders for it: private files, liveness lock, capped streams, retention
│  │  └─ crastu-redact/      redaction of known secret shapes in log text, plain-name validation
│  ├─ engines/
│  │  ├─ crastu-audio/       capture seam (AudioSource: file, synthetic, cpal mic behind `mic`), 16 kHz converter, level meter, ring buffer, VoiceGate; playback and system audio later
│  │  ├─ crastu-stt/         trait Transcriber; adapters: sherpa-onnx, whisper.cpp, cloud (BYOK)
│  │  ├─ crastu-tts/         trait Synthesizer; adapters: built-in, local server, cloud, command
│  │  ├─ crastu-llm/         trait TextModel; adapters: Anthropic, OpenAI-compatible (OpenAI, OpenRouter, Groq, Gemini, custom)
│  │  ├─ crastu-media/       FFmpeg (LGPL, dynamically linked): probe, decode, encode, waveforms, thumbnails
│  │  ├─ crastu-ml/          model session registry: one session per (model, options), counted handles, idle expiry
│  │  ├─ crastu-vision/      face detection (YuNet), scene cuts, tracking, crop-path smoothing
│  │  ├─ crastu-captions/    caption model, styles, word animation, renderer
│  │  └─ crastu-sidecar/     process supervisor and length-prefixed JSON protocol for command engines
│  ├─ integrations/
│  │  ├─ crastu-mcp/         rmcp router; tools contributed by modules; stdio and optional local HTTP
│  │  ├─ crastu-memory/      uteke-core adapter (embedded, local)
│  │  └─ crastu-sync/        optional Uteke Cloud HTTP client (off by default)
│  ├─ os/
│  │  ├─ crastu-os-input/    global hotkeys, push-to-talk, text insertion, focus target
│  │  ├─ crastu-os-perms/    permission status and guides (TCC on macOS)
│  │  └─ crastu-crash/       fatal-signal handler writing crash.txt into the recorder's session folder (Unix)
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
│     ├─ crastu-agents-model/ Settings, Agents as plain data: pairings, policy, token reveal, button rules, `AgentsPlatform` (no GPUI)
│     ├─ crastu-agents-ui/   the Settings, Agents (MCP) page (pairing, token box, policy)
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

**Session registry (`crastu-ml`, issue #26).** `Registry<F: SessionFactory>` keeps one session per (model file identity, options) per process. `load` returns a counted `Handle`; concurrent loads of one key create one session (the factory runs without the registry lock, and a failed or panicking create is not cached); dropping the last handle starts an idle timer on an injectable `Clock`, and the session is freed once it has been idle for the configured time, while a load inside that window reuses it. Options are the execution provider (CPU default, Core ML opt in), intra-op threads (default min(cores, 4)) and the graph optimisation level; `is_deterministic()` is true only for CPU, and caches must rely on results of deterministic sessions only. Expiry is lazy: it runs on `load`, on `Handle` drop, on `snapshot` and on `sweep`, and the crate has no timer of its own, so the embedder must call `Registry::sweep` periodically (for example from the host's job loop) for idle sessions to release their memory on a quiet app. `Registry::snapshot` reports sessions, handles per model and approximate memory for the flight recorder and Settings. The crate has no runtime dependency, so it compiles for iOS and Android; the `ort`-backed factory comes in a later slice and only covers sessions Crastu creates through Rust (decision 28).

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
- Settings, Agents (MCP): the screen lists the paired agents (name, state, created, last call from the activity log), pairs a new one by name (the host's name rules), shows the token once in a box with a Copy button and the instruction to put it in the client's `env` block as `CRASTU_AGENT_TOKEN`, never on the command line (shell history, process list), and rotates or revokes an agent (revoke asks first). The page state and the rules for each button are plain data in `crastu-agents-model` (re-exported as `crastu_shell::agents`; its `AgentsPlatform` trait is part of the shell's `Platform`); `crastu-agents-ui` only draws it and sends intents the owner applies against the current state. The desktop app implements the platform calls over the same `crastu.db` as the host and `crastu agents`. The token is held in the model only while the box is open: leaving the page, Done or revoking that agent drops it, and it reaches the clipboard only from the Copy button. The approval policy is stored in the `agent_policy` table (an empty agent name is the default of a call class, a name is that agent's override); the host reads it on every call, and when it cannot be read, or holds a row it does not understand, only Read calls go on. A damaged row does not take the list down: the snapshot flags it, the page disables the policy controls and offers a reset to the defaults (one transaction that first re-checks the row is still damaged, rolled back on any failure), and the host keeps allowing reads only until then. Auto for a default or for one agent's Deletions needs a confirmation in the model. A caller's declared name is cleaned (no control or bidi characters, 64 characters) at the model layer. On open the desktop app tightens an existing data folder to 0700 and database files to 0600. A second token cannot be issued while one is still on screen, and Pair and Rotate are disabled while it is. The page lists the newest calls of every caller, marking unpaired ones.
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
| UI | `gpui-component` 0.9.1 (GPUI Kit, Apache-2.0) pinned with `gpui-pre` 0.3.12, exact versions in the workspace manifest; upgrade weekly in lockstep. GPUI is a dependency on macOS and Windows only for now (Linux follows Windows), so the crates are empty on Linux and the Linux CI job needs no X11 or Wayland libraries. Metal shaders are compiled at launch (`runtime_shaders`) in CI and in release builds, so no build needs the Metal Toolchain (decision 32); precompiling is a later optimisation once the start-up cost is measured (#42). `deny.toml` lists the unmaintained-crate notices that come with GPUI, each with a reason and a review date |
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

`crastu-core::net` holds the one gate for outbound requests (rule 4 above). `NetGate` is cheap to clone and shares one switch; a caller asks `gate.check("model download whisper-small", url)` before opening a request and gets `Err(Offline { what, host })` when Offline is on, for example `Offline: model download whisper-small from huggingface.co refused`. Only the host appears in the message. The gate keeps the last 100 refusals (what, host, a sequence number) for `refusals()` and `drain_refusals()`, which the activity log will read. The switch is the setting `network.offline` (off by default) and the environment variable `CRASTU_OFFLINE`; `crastu-settings::offline_enabled` resolves them, with a non-empty environment value winning over the setting (an unrecognised value counts as on; empty or unset leaves the setting in charge). Without a data folder and without the variable, the CLI says it cannot locate the setting rather than assuming online. In `crastu-models`, `GatedFetch` wraps any `Fetch` and refuses before the inner adapter runs, so no socket is opened; a model that is already on disk and verified is used without any request, and `require_installed` says how to get a missing one. `crastu models download` and `crastu models list` build the gate from `settings.json` in the data folder and `CRASTU_OFFLINE`; the desktop app and the CLI share one lookup for that folder (`crastu_settings::data_dir`: `CRASTU_DATA`, else `~/Library/Application Support/Crastu` on macOS, `%APPDATA%\Crastu` on Windows, `$XDG_DATA_HOME/crastu` or `~/.local/share/crastu` elsewhere), so the setting is the same in both. The desktop app builds one gate at start (`desktop_gate` in `apps/crastu-desktop/src/gate.rs`, plain data and unit-tested) from the loaded settings and `CRASTU_OFFLINE`; when `settings.json` cannot be read the gate is Offline ON (unless the environment variable says otherwise) and the start-up toast says so. Settings > Privacy shows an Offline switch with its one-line explanation: a click sends `Action::ToggleOffline`, the model flips the value it holds, flips the shared gate at once and writes `network.offline` to `settings.json`; the command palette has "Turn Offline on" and "Turn Offline off". When `CRASTU_OFFLINE` is set the switch shows its state, is disabled and says "Set by the CRASTU_OFFLINE environment variable". The switch fails closed: when `settings.json` could not be read the switch can only keep Offline on (the corrupt file is never overwritten, the toast says to fix it and restart), and when saving fails a change toward off is rolled back, gate included, while a change toward on stays on and the save error is shown. While `CRASTU_OFFLINE` decides, the palette leaves the Offline commands out. `crastu host` (the CLI, and the desktop binary's `host` command) builds its gate with the same code as `crastu models` (`apps/crastu-cli/src/gate.rs`) and hands it to its `Host` with `Host::set_net_gate`; the desktop app itself has no in-process host. The host reads `network.offline` again before every tool call (the file is small, and a re-read has none of the gaps of an mtime check), so a change the desktop saves acts on a running host at its next tool call (work already running keeps the value it read; a change the desktop could not save, or a desktop locked by its own `CRASTU_OFFLINE`, does not reach a host started without it); when `CRASTU_OFFLINE` is set it decides for the whole run and the file is not read. A setting that cannot be read makes the host's gate Offline ON, reported once on stderr per problem, where `crastu models` stops with an error instead; without a data folder the host does not start. The built-in host tools (`project_list`, `job_status`, `job_cancel`) make no network request; a tool that will must call `check` on the shared gate. With a gate: every tool call drains the refusals made while it ran into the activity log as an entry for that agent and tool with the decision `refused: <what> from <host>` and the outcome `blocked`, and `Host::record_refusals(&gate)` logs refusals that no agent caused as `crastu` / `network`. The log holds the what and the host only, never a path, query or credentials (the gate does not keep them). The gate cannot tell who asked, so a refusal the app itself causes while an agent call is running may be logged against that call. The Windows data folder is `%APPDATA%\Crastu`; the `crastu-ipc` socket path follows its own rules. Every new request type (update check, providers, sync) must go through the same gate.

### Flight recorder

With no telemetry, a user's own log folder is the only evidence of a crash, so `crastu-recorder` keeps one per run: `<root>/<app>/<UTC start>-<pid>/` with `session.json` (app, version, OS, enabled modules) at start, `events.jsonl` (one JSON object per line, one unbuffered write each, so a crash loses nothing), `perf.jsonl` (the `PerfSample` schema: frame p50/p99/max, resident memory, queue depth, job timings), `panic.txt`, `session.lock` and `end.json`. A folder without `end.json` ended badly. `end.json` holds `status` (`ok`, `error` or `panicked`), `clean` (true only for `ok`), `panics` (every panic the hook saw, caught ones included), `dropped_lines` and `duration_ms`; `finish(Outcome)` writes it, a caught panic in a thread does not stop an `ok` end (it is counted in `panics`), a panic after `finish` rewrites it as `panicked`, and a panic out of `main` leaves none. The root is `CRASTU_LOG_DIR`, else `~/Library/Logs/Crastu` on macOS, `%LOCALAPPDATA%\Crastu\Logs` on Windows, `$XDG_STATE_HOME/crastu/logs` or `~/.local/state/crastu/logs` elsewhere (a pure function with an injected environment). Every file is created with mode 0600 in a 0700 folder. The app name must be a plain name (letters, digits, `_:-`), so it cannot leave the root.

The panic hook writes message, location, thread, backtrace and the last 100 events (kept in a 256-entry memory ring), then calls the previous hook; it contains nothing that can panic (a nested panic aborts), uses `try_lock`, and reads a poisoned lock anyway. A live session cannot fill the disk: `events.jsonl` stops at 20 MB and `perf.jsonl` at 10 MB (one marker line is written, later lines are dropped and counted in `dropped_lines`), and `panic.txt` holds the first five panics in full, then one line per later panic, and nothing once it is 256 KB (every panic is still counted in `panics`). If the folder cannot be created the recorder prints one `flight recorder disabled: ...` line on stderr and does nothing else; a failed write is counted and the stream is reopened on the next write.

Liveness and retention: each recorder holds an exclusive lock (`File::try_lock`) on `session.lock` for its lifetime, and the operating system drops it when the process ends however it ends. Where the file system has no locks (some network or FUSE homes) a conservative rule applies instead: a session younger than an hour is alive, an older one is dead only with an `end.json`, and one without it counts as alive for a week. At start the recorder prunes the app's older sessions newest first, stopping once either limit (20 sessions, 50 MB) is exceeded, so the first session that does not fit and every older one go, in folder-name order, touching only folders named like sessions. It never deletes the current session or one whose lock cannot be taken (a running host next to many short commands); such a session still counts toward the limits. A session that is not locked and has no `end.json` crashed or was killed, and is kept as evidence only inside the same limits as any other.

Job and MCP events take only a plain name (letters, digits, `_:-`, so never a path) and an enum (tool name and decision, never content), and `JobTiming` can only be made through a constructor that checks the name. The version and module names in `session.json` are checked the same way. Free-text log messages and the panic message are length-limited and have known key shapes redacted (`sk-`, `hf_`, `ghp_`, `github_pat_`, `xox*-`, AWS key ids, JWTs, `Bearer` and `Basic` values, `crastu_<hex>`, URL `user:pass@`, Stripe, Google, GitLab and npm keys, and the values of secret names in `name=value`, `name: value` and JSON `"name": "value"` forms with any whitespace around the separator; a name is split into words at `_`, `-`, `.` and camelCase boundaries and counts when a word is or ends with `token`, `secret`, `password`, `credential` and the like, or is `key` after `api`, `access`, `private`..., so `accessToken`, `client-secret`, `API_KEY` and `private_key` are covered, while a last word such as `count`, `type` or `size` (`token_count`) is not; plus header-like values (`Authorization`, `Cookie`, `Set-Cookie`, `x-api-key`, and any `token`, `secret` or `credential` name after a colon) to the end of the line, and the credential after a scheme word such as `Bearer`; escaped JSON (`{\"password\":\"x\"}`), shell flags (`--password x`, and `-p x` or `-px` after `mysql`, `psql`, `sshpass` and similar, where a bare `-p 8080` is left alone), whole PEM private key blocks (also with a literal `\n`, or to the end of the text when cut off), and Slack, SendGrid, Telegram and Stripe-style tokens; a very long name is judged by its last words; token counts (`max_tokens`, `prompt_tokens`) and `PWD` are not secrets; a value on the line after the separator is masked only when that line holds the value alone or its first word has 8 or more characters; `password=` with spaces in the value goes to the end of the line; an unquoted value that holds a quote or bracket (a Rust `Debug` wrapper such as `Some("...")`) is followed to its balanced close, or to the end of the line; arrays after a secret name are masked whole; a PEM key as a value goes whole; `curl -u user:pass`, `--user` and `--proxy-user` lose the credential; a header inside a quoted shell argument ends at its closing quote; short names `pass`, `pin`, `otp`, `sid`, `bearer`, `passcode`, `dbpass` and `session_id`/`JSESSIONID` count as whole words only; a count name (`max_tokens`, `token_count`, the count word last) is a count only while its whole value is a number, `null` or `unlimited`, so `token_count: 5 sk-...` is masked, and `access_tokens` only while it is digits; an unmatched closing bracket ends a plain value only when the value held brackets of its own, a quote ends a header only when it starts a shell argument or string (not the apostrophe in `don't`), and a token, secret or key name after a colon takes the whole line even when its value is a `Some(...)` wrapper); quoted values are masked to their closing quote (escapes honoured, to the end of the line if it never closes), an unquoted value is masked through the whole token even if it holds quotes, text over 64 KB is cut back to the last whitespace and marked `[truncated]` before redaction, so a token across the cut is dropped whole, and the work is linear in the input, which is a safety net and not a content filter: callers must not pass user content. Slice 1 wires it into `crastu-cli` only (`host`, `mcp`, `models`; the desktop binary runs the same commands through it). Later slices of issue #25: the GUI and host perf feed, and Settings, About with "Open logs" and "Copy diagnostics".

Fatal signals (slice 2): `crastu-crash`, an `os` crate and the only one on the crash path that allows `unsafe` (one `sys` module, Unix only, every block commented), writes `crash.txt` into the session folder when the process dies from SIGSEGV, SIGBUS, SIGILL, SIGFPE or SIGABRT: `signal` (name and number), `code` (`si_code`), `address` (the faulting address when the kernel raised the signal, else `none`), `pid`, `session` (the folder name) and `time` (UTC and Unix seconds). Fixed fields only, so no message, argument, environment or memory contents can reach it. `crastu-recorder` stays free of `unsafe`: it exposes `session_dir()`, and `crastu-cli` calls `crastu_crash::install(dir)` on the main thread right after the panic hook, for the same commands as the recorder. The handler is async-signal-safe: `install` prepares the absolute, NUL-terminated path and the session id in a leaked allocation; the handler formats into a 1 KB stack buffer with no-allocation integer and date helpers (safe code, unit-tested) and writes with `open(O_WRONLY|O_CREAT|O_EXCL|O_NOFOLLOW, 0600)`, `write` and `close`, so an existing file or a symlink is never written through. An atomic flag lets only the first fatal signal write; a later one (another thread, a fault in the handler) waits up to half a second (1 ms `nanosleep` steps) for that report to be written, then takes the default action. After writing, the handler calls the handler that was installed before it (if it was a function), restores the default action and raises the signal again, so the process dies by the signal and the system's crash report still happens. A crash leaves no `end.json`; `crash.txt` is the reason. `install` gives the calling thread an alternate signal stack (64 KB, or `SIGSTKSZ` if larger, mapped with `mmap` above a `PROT_NONE` guard page, as std does) when it has none. An install is all or nothing: if a step fails, the actions already replaced are restored and a later call may try again; only a full success makes later calls return `AlreadyInstalled`. Interplay with std: an unwinding panic raises no signal (`panic.txt` covers it), and a panic that aborts writes `panic.txt` and then `crash.txt` with SIGABRT. std installs a SIGSEGV/SIGBUS handler at start-up that recognises a guard-page hit, prints `thread '...' has overflowed its stack` and aborts, and gives every thread it starts an alternate stack; `install` saves that handler and chains to it, so a stack overflow writes `crash.txt` (SIGSEGV or SIGBUS), std prints its line, and the process dies by SIGABRT as before. A side effect worth knowing: without this crate, a SIGSEGV sent with `kill` to a Rust process is swallowed by std's handler (it restores the default action and returns, and nothing faults again); with it, the re-raise ends the process. Limits: a thread started by C code without an alternate stack gets `crash.txt` for an ordinary fault but not for a stack overflow; on macOS a signal sent with `kill` arrives with a fault-like `si_code` and address 0, which is written as it comes; a fault inside the handler itself delays the end by that half second; Windows (a structured-exception handler) comes later, and there `install` returns `Unsupported`. The desktop app's own window process has no recorder yet, so it gets the handler when it gets the recorder.

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
