# Crastu decision log

Owner decisions, newest last. Each one is also stored in the project memory (uteke).

| # | Date | Decision |
|---|---|---|
| 1 | 2026-11-10 | Licence Apache-2.0. Take concepts from VoiceStudio, OpenCut and OpenWhispr, never their code. |
| 2 | 2026-11-10 | Everything shipped or downloaded must allow commercial use, code and model weights. |
| 3 | 2026-11-10 | Free and personal-first; no monetisation work yet. |
| 4 | 2026-11-10 | MCP support is mandatory so external AI agents can operate the app. |
| 5 | 2026-11-10 | The editor includes an auto-clipper. |
| 6 | 2026-11-10 | Name: Crastu (from "Cora Studio"). Earlier names Gema and Cordio dropped; Cordio collides with Arm Cordio and Cordio Medical. |
| 7 | 2026-11-10 | Voice cloning: no built-in cloning for now. Users can add their own engines (for example OmniVoice); the UI labels them with their licence. |
| 8 | 2026-11-10 | TTS: Supertonic 3 presets for now. |
| 9 | 2026-11-10 | uteke is embedded as a library and serves as memory for the AI that helps manage the app. |
| 10 | 2026-11-10 | vetio: reuse what is needed; its licence question does not block Crastu. |
| 11 | 2026-11-10 | MCP approvals for changes and deletions are configurable, including bypass. |
| 12 | 2026-11-10 | Platforms: macOS first, then Windows; Linux later. |
| 13 | 2026-11-10 | Design system and UI/UX mockups come before any code. |
| 14 | 2026-11-10 | Visual: Graphite is the default; Graphite, Paper and Signal are user-selectable colour themes. Font pairing and density are separate settings. |
| 15 | 2026-11-10 | Layout: rail, left sidebar, content, right inspector. |
| 16 | 2026-11-10 | Language: discussion in Indonesian; UI, demos and documentation in English. |
| 17 | 2026-11-10 | LLMs are bring-your-own-key; no bundled LLM. |
| 18 | 2026-11-10 | Memory search: EmbeddingGemma 300M on device by default; provider embeddings optional. |
| 19 | 2026-11-10 | Design system v1 locked (see `DESIGN.md`). |
| 20 | 2026-11-10 | Architecture v2 approved for P0, starting with three spikes: shared ONNX Runtime, GPUI video on macOS, macOS hotkey and paste. |
| 21 | 2026-11-10 | Repository is private on the personal Gitea account (`ajianaz/crastu`), default branch `develop`. |
| 22 | 2026-11-10 | UI and logic are strictly separate: `crastu-app` holds view models; mobile can be added later via flutter_rust_bridge (or UniFFI). Core, app and domain crates must compile for iOS and Android. |
| 23 | 2026-11-10 | The repository is public on GitHub (`ajianaz/crastu`) and GitHub is the primary: free GitHub Actions minutes, including macOS runners. Gitea keeps a pull mirror as backup. Replaces decision 21. |
| 24 | 2026-11-10 | Roadmap: P0 Foundation, then P1 Dictation (v0.1, first release), P2 Voice, P3 Auto-clipper, P4 Editor, P5 Windows, then v1.0. Phases end on exit criteria, not dates. See `ROADMAP.md`. |
