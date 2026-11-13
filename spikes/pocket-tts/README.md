# Pocket TTS spike scripts (throwaway)

Not part of the workspace and not built by CI. They produced the numbers in `docs/spikes/pocket-tts.md`. Models and audio are never committed: download them into a fresh folder outside the repository (they are untrusted data) and pass paths as arguments.

- `bench_pocket.py MODELS_DIR LANG PRECISION VOICE LINES OUT_DIR`: runs the KevinAHM `pocket_tts_onnx.py` wrapper (revision `58a6d00c`, put it next to the models folder) with `onnxruntime`; `VOICE` is a WAV, or a `.npy` embedding saved by an earlier run. Run under `/usr/bin/time -l` for memory.
- `bench_sherpa.py supertonic|pocket MODEL_DIR LINES [REF_WAV]`: sherpa-onnx 2.0.1 Python wheel with the release archives named in the note.
- `align_eval.py LINES ASR_DIR NAME=PATTERN ...`: round-trip match and word-timing estimates against Parakeet TDT 110M int8 (needs `scipy`).
- `lines.txt`: the ten test lines.
