# Changelog

All notable changes are listed here, newest first. Versions stay below 1.0 until the v1.0 criteria of `docs/ROADMAP.md` hold; P0 produces no release, so everything so far is unreleased.

## Unreleased

- Foundation: workspace and layering checks, core crates, headless host with the stdio MCP shim and approval policy, `crastu-app` view models.
- UI: GPUI pinned and bundled fonts (Inter, JetBrains Mono), design tokens with a lint against literals, base controls with every state, badges, tooltip, toast and dialog, a component gallery.
- Shell: window frame with rail, sidebars and inspector, Appearance settings that persist, onboarding screens with real macOS permission checks (hands-on test open).
- Spikes: one shared ONNX Runtime (decision 28), video preview through VideoToolbox into `surface()` (decision 27).
- Licences: `cargo xtask licences` writes NOTICE and LICENSES/ for the shipped dependency graph and the bundled fonts, and `--check` runs in CI.
