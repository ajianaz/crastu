# Speech: speech-to-text, TTS and voice cloning

Research verified on 2026-11-10 (crate data from `https://crates.io/api/v1/crates/<name>`; READMEs and source via raw.githubusercontent.com). Access limits during that research: GitHub REST API, docs.rs, codeberg.org and gitlab.freedesktop.org were blocked by the proxy, so star counts are absent and a few last-commit dates fall back to crates.io release dates (marked).

Related notes: [auto-clipper.md](auto-clipper.md) (word timestamps, VAD, LLM models), [media-pipeline.md](media-pipeline.md) (audio I/O), [rust-ecosystem.md](rust-ecosystem.md) (dictation, hotkeys, Handy).

## Prior art

Every project consulted, on equal footing. "Concepts only" marks a project whose licence does not allow us to reuse its code. Decision 1 also keeps us to concepts for VoiceStudio, OpenWhispr and OpenCut. Licences were read from each GitHub repository on 2026-11-13 unless a version date is given in the tables below.

| Project | Licence | Concept taken | Link |
|---|---|---|---|
| whisper.cpp | MIT | Local Whisper inference with word-level timestamps | https://github.com/ggml-org/whisper.cpp |
| sherpa-onnx (k2-fsa) | Apache-2.0 | One engine for STT, VAD, TTS and in-process cloning, through the official Rust API | https://github.com/k2-fsa/sherpa-onnx |
| sherpa-rs | MIT (deprecated; the README points to the official bindings) | Nothing; superseded by the official crate | https://github.com/thewh1teagle/sherpa-rs |
| transcribe-rs (cjpais) | MIT | Several STT engines behind one Rust API | https://github.com/cjpais/transcribe-rs |
| Handy (cjpais/Handy) | MIT | Dictation architecture: supervised inference engine worker, VAD, model choice (see [rust-ecosystem.md](rust-ecosystem.md)) | https://github.com/cjpais/Handy |
| OpenWhispr | MIT | Dictation product concepts; code not used (decision 1) | https://github.com/OpenWhispr/openwhispr |
| VoiceStudio | AGPL (as recorded in ARCHITECTURE.md): concepts only | Voice studio product concepts | See [ARCHITECTURE.md](../ARCHITECTURE.md#1-product-scope) |
| Kokoros (lucasjinreal/Kokoros) | Apache-2.0 (the repository reports it; checked 2026-11-13, earlier unverified) | ort-based Kokoro TTS with CLI and OpenAI-compatible server | https://github.com/lucasjinreal/Kokoros |
| Piper (rhasspy/piper, archived) and its successor piper1-gpl | Piper MIT (archived); piper1-gpl GPL-3.0: concepts only (checked 2026-11-13) | VITS/Piper-style voices; not linked in-process | https://github.com/rhasspy/piper ; https://github.com/OHF-Voice/piper1-gpl |
| espeak-ng | GPL-3.0: concepts only | Phonemization; isolated in a sidecar process or avoided | https://github.com/espeak-ng/espeak-ng |
| Supertonic (supertone-inc) | Sample code MIT, model OpenRAIL-M; upstream archived (checked 2026-11-13) | Preset TTS voices (decision 8) | https://github.com/supertone-inc/supertonic |
| candle (huggingface) | MIT OR Apache-2.0 | Whisper and TTS examples (heavier, GPU-oriented) | https://github.com/huggingface/candle |

## Speech-to-text in Rust

| Crate | Latest | Released | License | Last commit | Notes |
|---|---|---|---|---|---|
| `whisper-rs` | 0.16.0 | 2026-03-12 | Unlicense | n/a (repo moved to codeberg.org/tazz4843/whisper-rs, which was blocked) | whisper.cpp bindings. ~8 months since last release. |
| `sherpa-onnx` (official) | 2.0.1 | 2026-10-27 | Apache-2.0 | k2-fsa/sherpa-onnx 2026-11-10 | **Official Rust API** from k2-fsa, plus `sherpa-onnx-sys` 2.0.1. Frequent releases (1.13.3 → 2.0.1 between Jun and Oct 2026). |
| `sherpa-rs` | 0.6.8 | 2025-10-05 | MIT | 2026-03-08 | **Deprecated**: README says to use the official sherpa-onnx Rust bindings. |
| `ort` | 2.0.0-rc.15 | 2026-10-19 | MIT OR Apache-2.0 | 2026-11-01 | Bindings to ONNX Runtime 1.31 (ort-sys description). Still release-candidate; MSRV 1.92 as of rc.14. 25.6M downloads. |
| `candle-core` / `candle-transformers` | 0.11.1 | 2026-10-31 | MIT OR Apache-2.0 | 2026-11-01 | Examples include `whisper`, `whisper-microphone`, `voxtral`, plus TTS `csm`, `parler-tts`, `metavoice`, `mimi`. |
| `transcribe-rs` | 0.3.12 | 2026-10-02 | MIT | 2026-10-03 | cjpais. Features: `onnx` (Parakeet, Canary, Cohere, Moonshine, SenseVoice, GigaAM via ORT), `whisper-cpp` (Metal/Vulkan/CUDA), `whisperfile`; ORT EPs cuda/rocm/directml/coreml/webgpu. |
| `transcribe-cpp` | 0.4.1 | 2026-10-22 | MIT | — | Bindings to handy-computer/transcribe.cpp (ggml-based STT). **Handy now uses this, not transcribe-rs.** |
| `parakeet-rs` | 0.4.4 | 2026-10-31 | MIT OR Apache-2.0 | 2026-10-31 | Parakeet via ort. |

The official sherpa-onnx Rust examples (`rust-api-examples/examples/`) cover ASR for whisper, nemo_parakeet, parakeet_tdt (simulated streaming mic), moonshine_v2, qwen3_asr, sense_voice, cohere_transcribe, fire_red_asr, funasr_nano, streaming zipformer/paraformer, punctuation, speaker diarization and embeddings, spoken language ID, speech enhancement (gtcrn, dpdfnet, voxlock), keyword spotting, plus **silero and TEN VAD**.
- `sherpa-onnx-sys/build.rs` downloads prebuilt libs from GitHub releases by default, with `static` and `shared` features and `SHERPA_ONNX_LIB_DIR` / `SHERPA_ONNX_ARCHIVE_DIR` overrides. Offline/CI builds should set these.

### VAD
| Crate | Latest | Released | License | Notes |
|---|---|---|---|---|
| `voice_activity_detector` | 0.2.1 | 2025-08-04 | MIT (LICENSE file; crates.io shows "non-standard" because it uses `license-file`) | Silero via ort; last commit 2025-08-04. |
| `earshot` (pykeio) | 1.2.2 | 2026-08-19 | MIT OR Apache-2.0 | "Ridiculously fast & accurate VAD in pure Rust"; no ONNX dependency. Used by Handy. |
| `silero-vad-rs` | 0.1.2 | 2025-04-04 | MIT | 3k downloads; avoid. |
| sherpa-onnx | — | — | — | Silero and TEN VAD built in. |

Sources: crates.io API for each; https://raw.githubusercontent.com/thewh1teagle/sherpa-rs/main/README.md ; https://raw.githubusercontent.com/cjpais/transcribe-rs/main/README.md ; https://raw.githubusercontent.com/k2-fsa/sherpa-onnx/master/sherpa-onnx/rust/sherpa-onnx-sys/build.rs ; git tree of k2-fsa/sherpa-onnx rust-api-examples ; git tree of huggingface/candle candle-examples ; https://raw.githubusercontent.com/nkeenan38/voice_activity_detector/main/LICENSE


## TTS and voice cloning in Rust

| Option | Version / date | License | Status |
|---|---|---|---|
| sherpa-onnx TTS (official crate) | 2.0.1 / 2026-10-27 | Apache-2.0 (code) | Rust examples: kokoro (en, zh+en), kitten, matcha, vits/piper-style, **supertonic_tts**, **pocket_tts (zero-shot cloning from a reference wav)**, **zipvoice (zero-shot cloning with reference audio + reference text)**. |
| Kokoros (lucasjinreal/Kokoros) | not on crates.io as `kokoros` (404) | Apache-2.0 (checked with the GitHub API on 2026-11-13) | Last commit 2026-10-05; ort-based; CLI + OpenAI-compatible server; README points to successor project "Crane". Use as a git dep or reference. |
| `kokoro-tts` (mzdk100) | 0.3.3 / 2026-06-01 | Apache-2.0 | Alternative Kokoro crate. |
| `piper-rs` | 0.2.0 / 2026-05-21 | MIT | Last commit 2026-05-21; depends on `espeak-rs` (espeak-ng). |
| Supertonic | upstream archived | sample code MIT; model OpenRAIL-M | supertone-inc/supertonic README: "This repository is archived. Development and support have ended." Weights moved to HF `supertone-oss-archive`. Still runnable through sherpa-onnx's `supertonic_tts` example. |
| candle | 0.11.1 | MIT/Apache | csm, parler-tts, metavoice examples (heavier, GPU-oriented). |

Model licenses: Kokoro-82M weights Apache-2.0 (https://huggingface.co/hexgrad/Kokoro-82M/raw/main/README.md). Piper upstream (`rhasspy/piper`) says "Development has moved: https://github.com/OHF-Voice/piper1-gpl" (new home is GPL-named; LICENSE not fetched).

**Licensing risk:** espeak-ng is GPL-3.0 (https://raw.githubusercontent.com/espeak-ng/espeak-ng/master/COPYING). Kokoro, Piper and VITS phonemization paths that link espeak-ng (piper-rs via espeak-rs, Kokoros' espeak tokenizer, sherpa-onnx "phonemize" variants) bring GPL obligations if statically linked. Prefer "lexicon" variants or a non-GPL G2P, or isolate espeak in a sidecar process. Which sherpa-onnx builds link espeak-ng: **unverified**.

Voice cloning:
- **In-process Rust, available now:** sherpa-onnx Pocket TTS and ZipVoice (both take reference audio; verified in `pocket_tts.rs` / `zipvoice_tts_lexicon.rs`). Pocket TTS model licence: CC-BY-4.0, checked on 2026-11-13 in `docs/spikes/pocket-tts.md` (the cloning weights are in a gated repository, two of the stock voices are non-commercial, and the sherpa-onnx archive README says "non-commercial" against its own CC BY 4.0 LICENSE file).
- **Python sidecar likely needed for:** state-of-the-art cloning stacks (F5-TTS, XTTS, CosyVoice, Chatterbox, etc.) that have no maintained ONNX/Rust port. This was not verified per model in this pass. Design a sidecar protocol (stdin/stdout JSON or local gRPC) so these can be added later.

