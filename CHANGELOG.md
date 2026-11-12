# Changelog

All notable changes are listed here, newest first. Versions stay below 1.0 until the v1.0 criteria of `docs/ROADMAP.md` hold; P0 produces no release, so everything so far is unreleased.

## Unreleased

- Build: decision 32 keeps `runtime_shaders` for CI and release builds (no Metal Toolchain needed); precompiled shaders wait for a start-up measurement (#42).
- ML: `crastu-ml` session registry (slice 1 of #26): one session per model and options per process, counted handles, idle expiry on an injectable clock, stats snapshot; no runtime dependency yet.
- MCP: agents can be paired with a token (`crastu agents add|list|revoke|rotate`, `CRASTU_AGENT_TOKEN` in the shim); per-agent policy overrides apply only to paired agents and the activity log marks unpaired calls.
- Foundation: workspace and layering checks, core crates, headless host with the stdio MCP shim and approval policy, `crastu-app` view models.
- UI: GPUI pinned and bundled fonts (Inter, JetBrains Mono), design tokens with a lint against literals, base controls with every state, badges, tooltip, toast and dialog, a component gallery.
- Shell: Cmd/Ctrl+K command palette (go to a module or a Settings category, toggle panels, switch theme, font and density); window frame with rail, sidebars and inspector, Appearance settings that persist, onboarding screens with real macOS permission checks (hands-on test open).
- Spikes: one shared ONNX Runtime (decision 28), video preview through VideoToolbox into `surface()` (decision 27).
- Licences: `cargo xtask licences` writes NOTICE and LICENSES/ for the shipped dependency graph and the bundled fonts, and `--check` runs in CI.
- Models: catalogue entries (Whisper Small, Silero VAD, YuNet) with pinned sha256 and size, a resumable verified downloader with an https adapter, `crastu models list|download`, and onboarding that reads its model table from the catalogue.
- Offline mode: one gate in `crastu-core` that network requests pass through, the `network.offline` setting and `CRASTU_OFFLINE`, model downloads refused by name while it is on (installed models keep working), and `crastu models list` showing the state. The desktop app and the CLI now share one data folder lookup; on Windows it is `%APPDATA%\Crastu` (pre-1.0, no migration), and the `crastu-ipc` socket path still follows its own rules.
- Build: the minimum Rust version is 1.89 (`File::try_lock`, used for the model download lock).
