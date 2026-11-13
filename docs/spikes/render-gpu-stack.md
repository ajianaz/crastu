# Render GPU stack: Vello and Parley for `crastu-render` slice 2

Note, not a spike with measurements. Read on 2026-11-13 from crates.io's API (`https://crates.io/api/v1/crates/<name>`) and the Vello and Parley READMEs on their default branch. No dependency is added and nothing was built or run. The follow-up with measurements is [`render-gpu-measurements.md`](render-gpu-measurements.md).

| Crate | Latest | Published | Licence | MSRV |
|---|---|---|---|---|
| `vello` | 0.12.0 | 2026-11-11 | Apache-2.0 OR MIT | 1.89 |
| `vello_cpu` | 0.4.0 | 2026-11-11 | Apache-2.0 OR MIT | 1.89 |
| `vello_hybrid` | 0.2.0 | 2026-08-07 | Apache-2.0 OR MIT | 1.88 |
| `parley` | 0.13.0 | 2026-11-12 | Apache-2.0 OR MIT | 1.88 |
| `fontique` | 0.13.0 | 2026-11-12 | Apache-2.0 OR MIT | 1.88 |
| `wgpu` | 30.0.2 | 2026-11-12 | MIT OR Apache-2.0 | 1.87 |

Findings:

- All are permissive and within the workspace's Rust 1.92. `vello` 0.12.0 depends on `wgpu ^30` (optional, no default features) and `skrifa ^0.49`, while `skrifa` is already at 0.50.0: the Linebender crates move together, so pin the set at one time. `media-pipeline.md` already lists `wgpu` 30.0.1 (GPUI on Linux uses a wgpu git revision: a version mismatch is possible if a device is shared with GPUI).
- Vello's README calls the CPU renderer "the most mature choice" and the GPU renderer "intended to become the primary renderer"; the compute renderer "remains an experimental implementation". Minor versions arrive about every one to two months (0.7.0 on 2026-01-13 to 0.12.0), so expect API churn on each bump.
- The README says nothing about headless rendering or determinism. Decision 39's test of identical frames on one machine must be run for real on the chosen backend; a CPU renderer (`vello_cpu`) is the likelier way to get byte-identical frames across machines and could back export while the GPU backs preview. Not measured.
- Parley is layout only (shaping through HarfRust, fonts through Fontique). The README does not discuss user-supplied fonts: whether fonts from `tokens.json` and user fonts register without the system font collection still has to be tried. Parley's own tests and benchmarks have separate licences; `parley_emoji` is MIT only.
- Open for slice 2: iOS and Android builds of the chosen backend through `cargo xtask mobile-check`, `cargo deny` on the new tree, and speed against the "60 s 1080x1920 faster than real time on an M1" goal of #35.
