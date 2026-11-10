# Research: "Cordio" name, auto-clipper pipeline, MCP in Rust (2026-11-10)

Markers: [V] verified against primary source in this session; [S] secondary source / search summary; [U] unverified (memory or not reachable from sandbox).

## 1. Name check: "Cordio"

| Where | Finding | Source |
|---|---|---|
| crates.io `cordio` | Not taken [V] | https://crates.io/api/v1/crates/cordio |
| npm `cordio` | Taken: placeholder v0.0.0 "Discord library code will be soon.", ISC [V] | https://registry.npmjs.org/cordio |
| PyPI `cordio` | Not found [V] | https://pypi.org/pypi/cordio/json |
| GitHub | ~45 repos match; most notable are Arm Mbed "Cordio" BLE repos (ARMmbed/cordio-ble-x-nucleo-idb0xa1, mbed-os-cordio-hci-passthrough); rest are tiny/personal [V] | https://github.com/search?q=cordio&type=repositories |
| Arm / Packetcraft | "Arm® Cordio®" Bluetooth LE stack, written with ® in Packetcraft press release, i.e. a registered mark of Arm in software. Open-sourced by Packetcraft 2019, integrated in Mbed OS; ADI MSDK ships a "Cordio" fork today [S] | https://prnewswire.com/news-releases/packetcraft-launches-open-source-bluetooth-low-energy-stack-which-is-integrated-with-arm-mbed-os-for-seamless-iot-connectivity-300931376.html ; https://analogdevicesinc.github.io/msdk/Libraries/Cordio/docs/CORDIO_USERGUIDE |
| Cordio Medical (Israel) | "Cordio HearO": smartphone **voice/speech-analysis** app for heart-failure monitoring; CE marked, raised $18M, US trials (Cleveland Clinic NCT05876533) [S] | https://calcalistech.com/ctechnews/article/hylittpq9 ; https://www.biotechniques.com/news/cordio-hearo-the-app-that-could-detect-heart-failure-by-the-sound-of-your-voice ; https://my.clevelandclinic.org/clinical-trials/1807-cordio-hearo-an-international-multicenter-observational-single-arm-blinded-study-to-assess-the-performance-of-the-cordio-hearo-system |
| App stores | iOS app "Cordio" exists (Cordio Medical's) [S] | https://appwisp.com/app/ios/cordio |
| Domains cordio.app/.dev/.io/.com | Could not check: sandbox proxy refuses CONNECT to these hosts [U] | — |
| Trademark registers (USPTO/EUIPO) | Not checked directly [U] | — |

**Severity: HIGH-MEDIUM.** Two established holders in adjacent spaces: Arm's registered "Cordio" mark in software (class 9) and Cordio Medical, whose product is literally a voice/speech-analysis software app. Our app does voice/speech software. For a free OSS app the practical risk is confusion/takedown on app stores and poor searchability, not huge damages, but it is avoidable.

**Alternatives (quick registry check, all [V] free on crates.io + npm, PyPI noted):**
- `corastudio` / `cora-studio`: free on crates.io, npm, PyPI. Web search found no product named "Cora Studio" [S] (search was shallow; "Cora" alone is a common name, e.g. other AI products may exist [U]).
- `reelvox`: free on crates.io, npm, PyPI; no search hits [S].
- `speakreel`: free on crates.io, npm, PyPI; no search hits [S].
- (`voxreel`: free on crates.io/npm but **taken on PyPI**.)
Run a USPTO/EUIPO and domain check before committing.

## 2. Auto clipper (long video -> vertical shorts)

### OSS implementations surveyed
| Project | License | Pipeline notes | Source |
|---|---|---|---|
| ClipsAI (clipsai) | MIT [V] | WhisperX word timestamps -> ClipFinder (transcript-based segmentation) -> resize 16:9->9:16 using **pyannote speaker-diarization-3.0** (HF-token gated) to follow the current speaker | https://raw.githubusercontent.com/ClipsAI/clipsai/main/README.md |
| SamurAIGPT/AI-Youtube-Shorts-Generator | MIT [V] | transcript -> LLM classifies content type -> chunk >30 min into overlapping 20-min windows -> LLM scores candidates 0-100 against "virality" rubric -> dedupe >50% overlap -> top N -> ffmpeg + OpenCV face tracking with motion smoothing | https://github.com/SamurAIGPT/AI-Youtube-Shorts-Generator |
| Cutawan (JeremySNR/cutawan) | MIT [V] | Most complete design. Electron/TS. Chunked Whisper (word-level timestamps required) -> LLM hook/build/payoff selection + end-extension pass -> editorial re-rank (hook, clarity, value, payoff) -> **UltraFace** face detect + scene-cut detect -> IOU tracking + interpolation -> **LR-ASD via ONNX Runtime** with MFCC audio features (in child process) -> vocal-energy "arousal" signal drives auto-zoom -> remove pauses/fillers, remap captions -> ASS karaoke captions via libass -> H.264/AAC, -14 LUFS | https://raw.githubusercontent.com/JeremySNR/cutawan/main/README.md |
| OpenClip (aionixos/openclip) | MIT [V] | FastAPI + Next.js; MediaPipe face tracking; ffmpeg astats RMS energy; LLM (Gemini/Ollama/OpenAI/Anthropic) hook detection; scene detection on roadmap only | https://github.com/aionixos/openclip |
| OpenShorts, OpenClipper, SupoClip, ClippedAI | not verified [U] | listed as alternatives | https://openalternative.co/openshorts ; https://sourceforge.net/p/openclipper/ |

### Step-by-step options runnable from Rust (commercial-safe)

1. **Transcript with word timestamps**
   - whisper.cpp (MIT [V]): word-level via `-ml 1` (marked "experimental"), `--split-on-word`, and DTW token timestamps (`--dtw`, `dtw_token_timestamps`) [V]. https://raw.githubusercontent.com/ggml-org/whisper.cpp/master/LICENSE ; https://github.com/ggml-org/whisper.cpp/blob/master/README.md ; https://github.com/ggml-org/whisper.cpp/blob/master/examples/cli/cli.cpp
   - Whisper weights: MIT (e.g. openai/whisper-large-v3-turbo) [V]. https://huggingface.co/openai/whisper-large-v3-turbo
   - sherpa-onnx (Apache-2.0 [V], crate `sherpa-onnx` 2.0.1 on crates.io [V]): Whisper path exposes **segment** timestamps (`enable_segment_timestamps`) [V]; token timestamps come from transducer/CTC models such as NVIDIA Parakeet-TDT-0.6B-v3 (CC-BY-4.0 [V], commercial OK with attribution) [token-timestamp detail U]. https://github.com/k2-fsa/sherpa-onnx/blob/master/sherpa-onnx/csrc/offline-recognizer-whisper-impl.h ; https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3
   - VAD: Silero VAD MIT [V]. https://raw.githubusercontent.com/snakers4/silero-vad/master/LICENSE

2. **Scene detection**
   - PySceneDetect BSD-3-Clause [V]; content detector (HSV frame-diff threshold) / adaptive detector are simple to reimplement in Rust. https://raw.githubusercontent.com/Breakthrough/PySceneDetect/main/LICENSE
   - TransNetV2 code MIT [V] (weights in repo; exportable to ONNX [U]). https://raw.githubusercontent.com/soCzech/TransNetV2/master/LICENSE

3. **Face detection**
   - YuNet (OpenCV zoo): "All files in this directory are licensed under MIT License" [V]; ONNX files incl. `face_detection_yunet_2026may.onnx` (dynamic input) and 2023mar int8 variants [V]. https://github.com/opencv/opencv_zoo/tree/main/models/face_detection_yunet
   - UltraFace (Ultra-Light-Fast-Generic-Face-Detector-1MB) MIT [V], used by Cutawan. https://raw.githubusercontent.com/Linzaer/Ultra-Light-Fast-Generic-Face-Detector-1MB/master/LICENSE
   - MediaPipe BlazeFace: MediaPipe code Apache-2.0 [V]; Qualcomm's HF re-export "MediaPipe-Face-Detection" tagged apache-2.0 [V]; Google's own model card license not fetched (ai.google.dev blocked) [U]. https://raw.githubusercontent.com/google-ai-edge/mediapipe/master/LICENSE ; https://huggingface.co/qualcomm/MediaPipe-Face-Detection
   - **AVOID SCRFD / InsightFace pretrained models**: code MIT, but training data and models trained on it are "for non-commercial research purposes only"; buffalo packs need licensing by email [V]. https://github.com/deepinsight/insightface

4. **Active speaker detection**
   - LR-ASD (IJCV 2025) MIT [V]; Light-ASD (CVPR 2023) MIT [V]; TalkNet-ASD MIT [V]. Weights have no separate license stated in the repos; they are trained on AVA-ActiveSpeaker (Google; annotations believed CC BY 4.0 [U], research.google.com blocked). Cutawan ships LR-ASD weights as MIT and exports to ONNX via `scripts/export-asd-onnx.py` [V]. Residual risk: dataset-derived weights; low but worth noting. https://raw.githubusercontent.com/Junhua-Liao/LR-ASD/main/LICENSE ; https://raw.githubusercontent.com/Junhua-Liao/Light-ASD/main/LICENSE ; https://raw.githubusercontent.com/TaoRuijie/TalkNet-ASD/main/LICENSE.md
   - Avoid pyannote diarization models used by ClipsAI (HF-gated; terms not checked here [U]).
   - Cheap fallback without a model: per-face mouth-region motion correlated with audio RMS (OpenClip does "audio RMS and lip signals") [S].

5. **Audio cues**: RMS / loudness (ffmpeg astats or own DSP), MFCC; laughter: jrgillick/laughter-detection MIT [V] (https://raw.githubusercontent.com/jrgillick/laughter-detection/master/LICENSE); YAMNet (AudioSet "Laughter" class) Apache-2.0 [U].

6. **Segmentation / virality scoring (LLM)**: OpenAI-compatible API (user key) or local llama.cpp (MIT [U]). HF license tags [V] via https://huggingface.co/api/models/<id>:
   - Qwen/Qwen3-4B, Qwen3-8B: apache-2.0, not gated
   - google/gemma-4-E4B-it: apache-2.0, not gated
   - google/gemma-3-4b-it: "gemma" license, gated (manual) -> custom terms with use policy; avoid under the hard rule
   - microsoft/Phi-4-mini-instruct: mit
   - mistralai/Ministral-3-8B-Instruct-2512: apache-2.0
   - HuggingFaceTB/SmolLM3-3B: apache-2.0
   - ibm-granite/granite-4.0-micro: apache-2.0

7. **Reframe crop path**: track faces (IOU + interpolation), pick target via ASD, smooth crop x(t) (Cutawan: interpolation/focus track; AI-Youtube-Shorts: "motion smoothing") [V]; hard-cut on scene boundaries. Filter choice (e.g. One-Euro/Kalman/critically damped spring) is our design decision, not sourced.

8. **Captions & export**: ASS karaoke + libass burn-in (Cutawan) [V]. libass is ISC [U]. FFmpeg must be an LGPL build (no `--enable-gpl`, no libx264) to respect the no-GPL-in-process rule, or run GPL ffmpeg as a separate process [U, general knowledge].

## 3. MCP in Rust

- `rmcp` latest **3.9.0**, published 2026-11-05, license **Apache-2.0**, MSRV (rust_version) **1.88** [V]. https://crates.io/api/v1/crates/rmcp/3.9.0
- Repo LICENSE: project transitioning MIT -> Apache-2.0; un-relicensed older contributions remain MIT [V]. https://raw.githubusercontent.com/modelcontextprotocol/rust-sdk/main/LICENSE
- Spec support: implements **2026-07-28**, compatible with 2025-11-25 and earlier; `ProtocolVersion::LATEST = V_2026_07_28`, `LATEST_WITH_INITIALIZE = V_2025_11_25` [V]. https://raw.githubusercontent.com/modelcontextprotocol/rust-sdk/main/README.md ; https://github.com/modelcontextprotocol/rust-sdk/blob/main/crates/rmcp/src/model.rs
- Transports: stdio (`transport-io`), child process client, Streamable HTTP server (Tower service, `transport-streamable-http-server`, `-session`), Streamable HTTP client (reqwest, also unix-socket variant), worker/in-process. Legacy 2024-11-05 HTTP+SSE not provided [V]. Feature list from crates.io API [V].
- Server features: `#[tool]`/`#[tool_router]`/`#[tool_handler]` macros, resources (templates, subscriptions), prompts, completions, progress/cancellation, elicitation (form + URL), tasks (SEP-2663), MRTR; sampling/roots/logging deprecated but functional; OAuth features (`auth`, client-credentials JWT, enterprise-managed) [V].
- **Current spec 2026-07-28** (folder present in spec repo alongside `draft`) [V]: removes `initialize` handshake and `Mcp-Session-Id` (stateless), adds `server/discover`, replaces GET stream / resources subscribe with `subscriptions/listen`, moves tasks to extension `io.modelcontextprotocol/tasks`, introduces MRTR replacing server-initiated sampling/elicitation requests, adds `resultType` [V]. https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/docs/specification/2026-07-28/changelog.mdx . Whether it is formally "released" vs RC not confirmed (modelcontextprotocol.io blocked) [U].
- Streamable HTTP security (2025-11-25 spec text): servers MUST validate `Origin` (403 if invalid) against DNS rebinding; SHOULD bind 127.0.0.1 not 0.0.0.0; SHOULD implement authentication [V]. https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/docs/specification/2025-11-25/basic/transports.mdx
- Clients: Claude Code `claude mcp add --transport stdio <name> -- <cmd>` and `claude mcp add --transport http <name> <url> --header "Authorization: Bearer ..."`; also `headersHelper` for dynamic headers; SSE transport deprecated [V]. https://code.claude.com/docs/en/mcp
- Claude Desktop: local servers via `claude_desktop_config.json` (stdio command) or Desktop Extensions (.mcpb); custom "connectors" for remote HTTP are brokered from Anthropic's cloud, so a localhost HTTP endpoint is likely NOT reachable that way [U].

### Architecture options for a desktop app
- **A. stdio shim (`cordio-mcp` CLI)**: client spawns the CLI; it connects to the running GUI over local IPC (Unix domain socket / Windows named pipe, per-user perms) and proxies tool calls; can launch the app if not running. Works with every client incl. Claude Desktop config. rmcp: server on `stdio()`; IPC via tokio + your own protocol, or `transport-streamable-http-client-unix-socket` toward an in-app HTTP-over-UDS server [feature name V, pattern is design].
- **B. Streamable HTTP on 127.0.0.1**: app hosts rmcp Tower service (axum) on loopback, random or fixed port, validates Origin, requires bearer token stored in a user-only file shown in Settings; Claude Code connects via `--transport http --header`. Simpler, no shim, but Claude Desktop needs a shim/bridge [U for Desktop].
- Recommended: ship both, sharing one tool router; stdio shim as the default integration.
