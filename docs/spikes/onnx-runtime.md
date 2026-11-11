# Spike: one ONNX Runtime for sherpa-onnx and uteke-core

Issue #2. Run on 2026-11-11. Code: [`spikes/onnx-runtime`](../../spikes/onnx-runtime) (standalone crate, not part of the workspace).

## Question

sherpa-onnx (speech to text, and later text to speech) ships its own ONNX Runtime. uteke-core (memory, EmbeddingGemma 300M) loads one at run time through the `ort` crate (`load-dynamic`). Can both live in one process, can they share one runtime, and what does sharing buy?

## Result in short

- **Both work together in one process in every combination tried**, on macOS arm64: no crash, no wrong output. The Whisper transcript of a test file was identical on a second run after uteke had loaded its runtime, and uteke's recall returned the right memory every time.
- **They can share one `libonnxruntime.dylib`.** sherpa-onnx with its `shared` feature links the dylib; uteke is pointed at the same file with `ORT_LIB_PATH`. The process then holds exactly one runtime image. uteke ran correctly on ONNX Runtime 1.28.2 (its own tests refer to 1.25.0), so the API level its `ort` crate asks for (18) is available.
- **What sharing saves is installed size, not much memory.** About 74 MB (sherpa with its runtime built in, plus uteke's own dylib) against about 46 MB (one dylib, sherpa as a small library): 28 MB less to ship. Resident memory was about 28 MB lower at the median over seven runs each (704 against 676 MB), but the ranges overlap and the whole process is dominated by the two models (about 600 to 700 MB).
- **Recommendation: ship one shared runtime.** sherpa-onnx `shared`, `libonnxruntime.dylib` from its release archive, uteke through `ORT_LIB_PATH` set by the host. The static layout also works and stays the fallback. The out-of-process embedder and provider embeddings, the other fallbacks named in the issue, were not needed.
- **Things to handle when packaging** are listed below: the run-time search path, the build-time download, the model download by uteke, and the signing of the libraries.

## Setup

| Item | Value |
|---|---|
| Machine | Mac mini (Mac14,3), Apple M2, 8 GB, macOS 26.5.1 |
| sherpa-onnx | crate 2.0.1; its release archives bundle ONNX Runtime **1.28.2** (the version string is in both the shared and the static archive; shared: `libonnxruntime.dylib` 27.4 MB, `libsherpa-onnx-c-api.dylib` 3.9 MB; static: the runtime is inside the executable) |
| uteke-core | 0.23.0 with default features; `ort` 2.0.0-rc.15 with `load-dynamic` and `api-18`; finds its runtime through `ORT_LIB_PATH`, its model through `UTEKE_HOME` |
| uteke's own runtime | ONNX Runtime 1.25.0 from the Microsoft release (35.0 MB dylib) |
| Whisper | `sherpa-onnx-whisper-tiny.en` (int8), 2 threads, a 6.6 s test file; transcript matches the reference apart from case and punctuation |
| EmbeddingGemma | uteke's own download (Q4, 187.6 MB plus 19.4 MB tokenizer), five sentences stored, one query recalled |
| Measurement | `ps` resident size after each step and the peak from `getrusage`; one process per run, fresh store each time |

## Results

One process per configuration runs: Whisper, then uteke (five stores and a recall), then Whisper again.

| Configuration | Runtime images in the process | Median peak (min to max), 7 runs | Whisper | uteke store + recall |
|---|---|---|---|---|
| **A** sherpa static (its own 1.28.2 inside the executable) + uteke with 1.25.0 | the 1.25.0 dylib, plus the one inside the executable | 704 MB (622 to 711) | 272 ms | 700 ms + 6 ms |
| **B** sherpa shared + uteke with **the same** 1.28.2 dylib | one `libonnxruntime.dylib` | 676 MB (628 to 684) | 241 ms | 685 ms + 8 ms |
| B2 sherpa shared + uteke with 1.25.0 | two dylibs with the same file name | 711 MB (one run) | 242 ms | 684 ms + 7 ms |
| C sherpa static + uteke with the 1.28.2 dylib | the 1.28.2 dylib, plus the one inside the executable | 699 MB (one run) | 272 ms | 692 ms + 12 ms |
| uteke alone (1.25.0) | one dylib | 394 MB (one run) | - | 893 ms + 30 ms |
| Whisper alone, static / shared | none / one | 240 / 244 MB (one run each) | 295 / 255 ms | - |

Notes on reading the table:

- **Memory varies a lot between runs of the same configuration** (A: 568 and 708 MB in two single runs; the seven-run set spans 622 to 711 MB). Only the seven-run medians of A and B are worth comparing, and even they overlap. Treat the saving as small, not as measured to the megabyte.
- Timing differences between A and B are within what repeated runs of one configuration show; they were not investigated.
- The `ps` resident size counts file-backed pages, so it is not the footprint the system would reclaim. It is the same measure for every row.
- Installed size: A is the 39.1 MB executable plus uteke's 35.0 MB runtime (74.1 MB). B is a 14.3 MB executable plus the 3.9 MB sherpa library and the 27.4 MB runtime (45.6 MB). The executables contain the spike and uteke, so only the difference between them (24.8 MB, the runtime built into A) is meaningful.

## Findings

1. **ONNX Runtime 1.28.2 is enough for uteke.** Configuration B and C ran uteke on 1.28.2 and returned the right top result; the `ort` crate asks for API 18, which 1.28 provides. This was a smoke test (open, five stores, one recall), not uteke's test suite, so a runtime upgrade still needs uteke's own tests.
2. **No clash between two runtimes in one process** (A, B2, C), including two dylibs with the same file name from different folders (B2). The cause was not investigated. This says nothing about Linux or Windows, where symbol resolution differs; not tested.
3. **The run-time search path is not inherited.** `sherpa-onnx-sys` emits an rpath for itself, but link arguments from a dependency's build script do not reach the final executable: the first `shared` build failed with `Library not loaded: @rpath/libsherpa-onnx-c-api.dylib … no LC_RPATH's found`. The application needs its own `build.rs` (the spike has one, about ten lines) that adds the library folder, and a packaged app a relative one such as `@executable_path/../Frameworks`. `install_name_tool` cannot add it afterwards because the header has no room.
4. **The build downloads from the network.** Reading its build script: unless `SHERPA_ONNX_LIB_DIR` points at an unpacked archive, `sherpa-onnx-sys` fetches its prebuilt libraries from GitHub releases while compiling (every build here set that variable, so the download itself was not run). CI and release builds should fetch them from a pinned URL with a checksum, and build offline.
5. **uteke downloads its model by itself.** On first use it fetches the EmbeddingGemma files from Hugging Face into `UTEKE_HOME/models/embeddinggemma-q4` (checksums are pinned in the crate). `ARCHITECTURE.md` lists that model as conditional (Gemma Terms), and the rule is that downloads go through the model catalogue with its licence badge. Whether uteke can be told not to download, so that `crastu-models` places the files, was not tested.
6. **Mobile.** The build script of `sherpa-onnx-sys` forces the shared mode on iOS and Android ("only support shared linking"), so the shared layout is the one that is the same everywhere. No mobile build was made here, and how uteke's runtime loading works on iOS was not looked at.

## What this means for the plan

- `crastu-host` sets `ORT_LIB_PATH` to the bundled runtime before the memory store opens, and the app bundle carries `libonnxruntime.dylib` and `libsherpa-onnx-c-api.dylib` in `Frameworks/`.
- Issue #26 (one session per model per process) can only share the sessions it creates itself through Rust; sessions that sherpa-onnx creates inside its C++ code stay its own. The registry still guarantees one runtime, one thread budget and one place to choose the execution provider for everything made through Rust.
- The libraries inside the app bundle have to be signed together with the app. Not tested here; check it when packaging starts (P1).

## Follow-ups

1. Decide on the shared layout (decision 28) and add the rpath and `ORT_LIB_PATH` handling to `crastu-host` and the packaging step.
2. Pin the sherpa archive by checksum for CI (finding 4), and find out whether uteke can skip its own download (finding 5).
3. Repeat on Windows and Linux before P5; try an iOS build of the shared layout before the mobile work.
4. Re-run with Whisper small and a real recording before P1 sets its latency target.

## Reproduce

```sh
# unpack the sherpa-onnx v2.0.1 osx-arm64 shared archive and the Whisper model, then:
cd spikes/onnx-runtime
SHERPA_ONNX_LIB_DIR=<archive>/lib cargo build --release --no-default-features --features shared
ORT_LIB_PATH=<archive>/lib/libonnxruntime.dylib UTEKE_HOME=<empty folder> \
  target/release/spike-onnx-runtime both <whisper model folder> <wav> <empty store folder>
# the static layout: --features static, and ORT_LIB_PATH at any other libonnxruntime.dylib
```

The program prints one JSON line with the resident size after each step and the runtime images in the process.
