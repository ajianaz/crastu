# Spike: Firewheel or our own small graph for the mixer (issue #33)

Run on 2026-11-13. Sources are crates.io and the Firewheel repository, read on that day.

## Question

The mixer (voices, music, gain ramps, ducking, bus sum, true-peak limiter, loudness) has to run in the output device callback for preview and offline for export, from one code path, with the same result whatever the block size. Is Firewheel the right base, or is our own small graph?

## What Firewheel is (read on 2026-11-13)

| Item | Value | Source |
|---|---|---|
| Latest release | `firewheel` 0.14.0, published 2026-09-05; last commit on `main` 2026-11-01 | crates.io API, GitHub commits API |
| Licence | MIT OR Apache-2.0 (all three core crates) | crates.io API |
| MSRV | not declared (`rust_version` is empty for `firewheel`, `-core`, `-graph`, `-nodes`) | crates.io API |
| Required dependencies of `firewheel-core` | `arrayvec`, `audioadapter`, `audioadapter-buffers`, `audioadapter-sample`, `bevy_platform`, `bitflags`, `firewheel-macros`, `num-traits`, `portable-atomic`, `ringbuf`, `smallvec`, `thiserror`, `thunderdome` | crates.io API |
| Required dependencies of `firewheel-graph` | the above plus `triple_buffer` | crates.io API |
| Backends | cpal, rtaudio and symphonium are optional crates; the README lists Windows, macOS, Linux, Android, iOS and WebAssembly backends | README |
| Scope | "does NOT aim to be a complete DAW engine"; the maintainer is moving it into the Bevy engine and describes feature creep he wants to cut | README |
| Built-in loudness, true-peak limiting, sidechain ducking | none found in the README's feature list | README |

## What was and was not checked

Checked: metadata, licence, dependency list, scope, activity. Not done: building Firewheel for the four mobile targets, and running its graph without a device. The README says the processing context is driven by a backend stream; I did not confirm that an offline "render N frames" driver exists, nor that two block sizes give bit-identical output (nodes that smooth parameters per block are the usual cause of block-size dependence). Those two are exactly what #33 needs, and they are not shown, so they are open for Firewheel, not failed.

## Decision

Our own small graph, in `crates/engines/crastu-mix`, std only, safe code.

- The requirement that matters most (bit-identical output for any block size, one code path for live and offline) is easiest to guarantee when every stage is either a pure function of the integer sample position or a per-sample causal filter. That is cheap to build for a fixed topology and hard to retrofit onto a general engine whose offline behaviour is unverified.
- The parts that are the product (ducking from the timeline, K-weighted loudness with gating, a true-peak limiter, gain to target) are not in Firewheel's feature list, so they would be written either way; the graph itself is a few hundred lines for a fixed topology (voice bus, music bus, sum, limiter).
- Thirteen required dependencies (and the Bevy platform crate) on the mobile path, and an undeclared MSRV against our 1.92 pin, are cost without a matching gain.
- Firewheel stays a candidate for the device-callback side (its cpal and mobile backends) if hand-written callbacks prove costly: `Mixer::process` has the shape of a callback (no allocation, no locks), so it can run inside any backend.

Revisit if the mixer needs user-built node graphs (plugins, per-clip effect chains); a fixed voice and music topology does not.
