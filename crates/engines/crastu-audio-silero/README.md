# crastu-audio-silero

The Silero VAD voice gate for `crastu-audio` (part of #85), split off to keep both crates under the size limit.

- `SileroGate<P: Predictor>`: a `crastu_audio::VoiceGate` on a speech probability. Frames of 320 samples (20 ms) go through a carry buffer into the model's windows of 512 samples (32 ms at 16 kHz); each frame is answered with the newest probability. A `threshold` (default 0.5) opens the gate and a lower release level (`threshold - release_margin`, default 0.15, floor 0.01) keeps it open, with the energy gate's 15-frame hangover; the start, continue and hangover code is `crastu_audio::Hysteresis`, shared with `EnergyGate`. A failing predictor counts an error and reads as probability 0.
- `Predictor`: one window in, one probability out, with its own state between windows. The gate logic is tested with a scripted fake and needs no model or runtime.
- Feature `ort` (off by default, so the default build and `cargo xtask mobile-check` stay free of ONNX Runtime): `OrtPredictor` and `silero_gate(&registry, &model_id, options, config)` on the `crastu-ml` registry. Register the `silero_vad.onnx` of the model catalogue on the `OrtFactory` first (`register_model`); every gate on the model then shares one session, and the state (2x128 floats) and the 64-sample context are per predictor.

The model's signature (checked 2026-11-13 from the ONNX file and `utils_vad.py` of the catalogued revision, which agree): inputs `input` f32 `[batch, 64 + 512]`, `state` f32 `[2, batch, 128]`, `sr` int64 scalar (16000); outputs `output` f32 `[batch, 1]` and `stateN` f32 `[2, batch, 128]`.

## Real-model tests

They are `#[ignore]` and need:

- `CRASTU_TEST_ORT_DYLIB`: the ONNX Runtime library (see `crates/engines/crastu-ml/README.md`);
- `CRASTU_TEST_ONNX_MODEL`: `silero_vad.onnx`, from `crastu models download silero-vad --dir <folder>` (the catalogue pins its sha256);
- `CRASTU_TEST_SPEECH_WAV`: a 16 kHz mono 16-bit WAV with speech and pauses. The clip used so far is `tests/data/test.wav` of the MIT-licensed snakers4/silero-vad repository at the catalogued revision (kept in a temp folder, never committed).

```sh
CRASTU_TEST_ORT_DYLIB=... CRASTU_TEST_ONNX_MODEL=... CRASTU_TEST_SPEECH_WAV=... \
  cargo test -p crastu-audio-silero --features ort -- --ignored --test-threads=1
```

`probabilities_match_the_reference_run_on_a_fixed_signal` needs no clip: it compares twelve windows of a synthetic signal with values from a Python `onnxruntime` run of the model, and fails if the carried state or the 64-sample context is dropped. `print_probabilities_for_comparison` prints the first 80 probabilities of the clip, for a comparison with such a run.
