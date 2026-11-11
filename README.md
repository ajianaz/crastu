# Crastu

[![ci](https://github.com/ajianaz/crastu/actions/workflows/ci.yml/badge.svg?branch=develop)](https://github.com/ajianaz/crastu/actions/workflows/ci.yml)
[![licence](https://img.shields.io/badge/licence-Apache--2.0-blue)](LICENSE)

A personal voice and video studio for macOS, then Windows.

- **Dictation**: hold a key, talk, and clean text lands in any app.
- **Voice**: turn a script into a voiceover with preset voices or engines you add.
- **Auto-clipper**: find the best moments in a long video and reframe them into short clips with captions.
- **Editor**: a timeline for clips, voice, music and captions.
- **Memory and agents**: a local memory (uteke) and a built-in MCP server, so AI agents can work with your projects under the limits you set.

Built in Rust with GPUI. Runs locally, sends no telemetry, and language models are bring-your-own-key.

## Status

Foundation phase (P0), not usable yet. Design system v1 is locked and architecture v2 is approved. The workspace, layering checks and core crates (ids and timecode, module registry, store, settings, jobs, model catalogue) are in place, along with UI-neutral view models in `crastu-app`. Progress is tracked in the [P0 epic](https://github.com/ajianaz/crastu/issues/8).

| Document | Content |
|---|---|
| [docs/DESIGN.md](docs/DESIGN.md) | Design system v1: principles, tokens, components, screens |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Crates, seams, MCP, data flows, mobile readiness, roadmap |
| [docs/DECISIONS.md](docs/DECISIONS.md) | Decision log |
| [docs/research/](docs/research/) | Research notes with sources |

## Development

```sh
cargo build --workspace
cargo xtask deps-check     # layering rules
cargo xtask build-matrix   # each module builds alone
cargo xtask mobile-check   # portable crates build for iOS and Android
```

Connect an AI agent (the shim starts the headless host when needed):

```sh
cargo build -p crastu-cli
claude mcp add --transport stdio crastu -- /path/to/target/debug/crastu-cli mcp --agent claude-code
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for the workflow and [AGENTS.md](AGENTS.md) for the full checklist and rules. Report security issues as described in [SECURITY.md](SECURITY.md).

## Licence

Apache-2.0. Every model Crastu ships or downloads allows commercial use; models with extra terms are labelled "Conditional" in the app.
