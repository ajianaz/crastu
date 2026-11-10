# Working on Crastu

Read `docs/ARCHITECTURE.md` and `docs/DESIGN.md` before changing code.

## Layers

Every crate declares `[package.metadata.crastu] kind` (core, engine, integration, os, feature, app, ffi, ui, feature-ui, bin, tool). Feature crates also declare `group`, and composite modules list the groups they may use in `composite`. `cargo xtask deps-check` enforces:

- Feature modules never depend on each other, except through `composite`.
- Only `ui`, `feature-ui` and `bin` crates may depend on GPUI.
- UI crates render `crastu-app` state and hold no business state.
- Core, engine, integration, os, feature, app and ffi crates compile for iOS and Android.

## Before every commit

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo xtask deps-check
cargo xtask build-matrix
cargo xtask mobile-check
cargo deny check
```

## Rules

- Everything Crastu ships or downloads must allow commercial use. New model entries need a licence id and a commercial flag.
- No telemetry. Network calls only for model downloads, update checks and providers the user adds.
- `unsafe` is denied workspace-wide; FFI crates opt in explicitly and document why.
- Git: branch from `develop` with `feat/`, `fix/`, `docs/` or `chore/`; Conventional Commits; squash merge; no AI attribution lines in commits or pull requests.
- Repository files are in English.
