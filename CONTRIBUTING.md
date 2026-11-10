# Contributing to Crastu

Thanks for your interest. Crastu is in its foundation phase (P0), so the structure still moves. Open an issue before starting anything larger than a small fix, so we can agree on the approach first.

## Before you start

- Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the crate layout and layering rules, and [docs/DESIGN.md](docs/DESIGN.md) for anything visible.
- [AGENTS.md](AGENTS.md) holds the full checklist. It applies to people and coding agents alike.
- Everything Crastu ships or downloads must allow commercial use. Do not add a dependency, model or asset with a non-commercial licence.
- Take ideas from other projects, never their code.

## Workflow

1. Branch from `develop`. Branch names start with `feat/`, `fix/`, `docs/`, `chore/`, `refactor/`, `perf/`, `test/`, `ci/`, `build/` or `release/`; CI rejects other names.
2. Write commits as [Conventional Commits](https://www.conventionalcommits.org/) (`feat(host): …`, `fix(store): …`).
3. Run the checks below before pushing.
4. Open a pull request into `develop` using the template (What, Why, Testing) and link the issue (`Closes #n`).
5. If `develop` moves on, rebase onto it (no merge commits) and push with `--force-with-lease`.
6. Pull requests are squash-merged once CI and the Cora review pass.

## Checks

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo xtask deps-check
cargo xtask build-matrix
cargo xtask mobile-check
cargo deny check
```

`mobile-check` needs the `aarch64-apple-ios` and `aarch64-linux-android` targets (`rustup target add …`). `cargo deny` comes from `cargo install cargo-deny`.

## Reporting problems

- Bugs and feature ideas: [open an issue](https://github.com/ajianaz/crastu/issues/new/choose).
- Security issues: see [SECURITY.md](SECURITY.md); do not open a public issue.

By contributing you agree that your work is licensed under Apache-2.0, the licence of this repository.
