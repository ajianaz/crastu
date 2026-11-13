# Auto-clipper: long video to vertical shorts

Researched 2026-11-10. Markers: [V] verified against a primary source in that session; [S] secondary source or search summary; [U] unverified (memory, or not reachable from the sandbox).

Related notes: [speech.md](speech.md) (transcription, VAD), [media-pipeline.md](media-pipeline.md) (FFmpeg, codecs), [rust-ecosystem.md](rust-ecosystem.md).

## Prior art

Every project consulted, on equal footing. Crastu takes concepts only from all of them (decisions 1 and 35). The licence column says what the project's repository declares; "concepts only" marks a project whose licence does not allow us to reuse its code.

| Project | Licence | Concept taken | Link |
|---|---|---|---|
| ClipsAI (clipsai) | MIT [V] | Word timestamps, then transcript-based clip finding, then a 16:9 to 9:16 resize that follows the current speaker | https://github.com/ClipsAI/clipsai |
| AI-Youtube-Shorts-Generator (SamurAIGPT) | MIT [V] | An LLM scores candidate segments against a rubric over overlapping windows, dedupe by overlap, top N; face tracking with motion smoothing | https://github.com/SamurAIGPT/AI-Youtube-Shorts-Generator |
| Cutawan (JeremySNR/cutawan) | MIT [V] | The most complete pipeline: hook/build/payoff selection, editorial re-rank, face and active-speaker tracking, vocal-energy auto-zoom, pause and filler removal, karaoke captions | https://github.com/JeremySNR/cutawan |
| OpenClip (aionixos/openclip) | MIT [V] | Audio RMS energy and face tracking as hook signals, LLM hook detection | https://github.com/aionixos/openclip |
| OpenShorts, OpenClipper, SupoClip, ClippedAI | not verified [U] | Listed as alternatives only; nothing taken | https://openalternative.co/openshorts ; https://sourceforge.net/p/openclipper/ |
| Building blocks: whisper.cpp, sherpa-onnx, Silero VAD, PySceneDetect, TransNetV2, YuNet, UltraFace, MediaPipe, LR-ASD, Light-ASD, TalkNet-ASD, laughter-detection, YAMNet | MIT, Apache-2.0 or BSD-3-Clause, each checked in the steps below | The models and libraries a Rust pipeline can run (steps 1 to 6) | Links in each step below |
| SCRFD / InsightFace pretrained models, pyannote diarization models | Code MIT, but models and training data non-commercial (InsightFace) [V]; pyannote terms not checked [U] | Concepts only: not used and not shipped, because the weights do not allow commercial use | https://github.com/deepinsight/insightface |

Licence check, 2026-11-13: the four verified projects report MIT on their GitHub repositories. The models and weights each pipeline uses carry their own licences, listed in the steps below.

## OSS implementations surveyed

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

## Footage ingest and export checks

These checks follow from the notes above and are collected in one place; nothing new was researched for this list.

- Ingest needs word-level timestamps from the transcript (step 1); Cutawan requires them for hook selection and caption remapping.
- Face and speaker models are the licence risk in this pipeline: avoid SCRFD / InsightFace pretrained models and the pyannote diarization models (steps 3 and 4).
- Keep the crop path smooth and hard-cut on scene boundaries (step 7); the filter is our own design decision.
- Export burns ASS karaoke captions with libass and targets H.264/AAC at -14 LUFS (Cutawan's numbers). The FFmpeg build must be LGPL, or GPL FFmpeg must run as a separate process (step 8 and [media-pipeline.md](media-pipeline.md)).
