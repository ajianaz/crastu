# Crastu

A personal voice and video studio for macOS, then Windows.

- **Dictation**: hold a key, talk, and clean text lands in any app.
- **Voice**: turn a script into a voiceover with preset voices or engines you add.
- **Auto-clipper**: find the best moments in a long video and reframe them into short clips with captions.
- **Editor**: a timeline for clips, voice, music and captions.
- **Memory and agents**: a local memory (uteke) and a built-in MCP server, so AI agents can work with your projects under the limits you set.

Built in Rust with GPUI. Runs locally, sends no telemetry, and language models are bring-your-own-key.

## Status

Design phase. The design system v1 is locked; the architecture is a draft under review. No application code yet.

| Document | Content |
|---|---|
| [docs/DESIGN.md](docs/DESIGN.md) | Design system v1: principles, tokens, components, screens |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Crates, seams, MCP, data flows, roadmap (draft) |
| [docs/DECISIONS.md](docs/DECISIONS.md) | Decision log |
| [docs/research/](docs/research/) | Research notes with sources |

## Licence

Apache-2.0. Every model Crastu ships or downloads allows commercial use; models with extra terms are labelled "Conditional" in the app.
