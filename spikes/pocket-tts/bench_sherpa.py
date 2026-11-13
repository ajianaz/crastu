"""Throwaway bench: sherpa-onnx 2.0.1 (Python wheel, CPU) with Supertonic 3 or Pocket TTS.

usage: bench_sherpa.py supertonic|pocket MODEL_DIR LINES [REF_WAV]
"""
import sys
import time
import json

import numpy as np
import soundfile as sf
import sherpa_onnx as so

kind, d, lines_path = sys.argv[1:4]
ref = sys.argv[4] if len(sys.argv) > 4 else None

t0 = time.perf_counter()
if kind == "supertonic":
    mc = so.OfflineTtsModelConfig(
        supertonic=so.OfflineTtsSupertonicModelConfig(
            duration_predictor=f"{d}/duration_predictor.int8.onnx",
            text_encoder=f"{d}/text_encoder.int8.onnx",
            vector_estimator=f"{d}/vector_estimator.int8.onnx",
            vocoder=f"{d}/vocoder.int8.onnx",
            tts_json=f"{d}/tts.json",
            unicode_indexer=f"{d}/unicode_indexer.bin",
            voice_style=f"{d}/voice.bin",
        ),
        num_threads=4, provider="cpu")
else:
    mc = so.OfflineTtsModelConfig(
        pocket=so.OfflineTtsPocketModelConfig(
            lm_flow=f"{d}/lm_flow.int8.onnx", lm_main=f"{d}/lm_main.int8.onnx",
            encoder=f"{d}/encoder.onnx", decoder=f"{d}/decoder.int8.onnx",
            text_conditioner=f"{d}/text_conditioner.onnx",
            vocab_json=f"{d}/vocab.json", token_scores_json=f"{d}/token_scores.json"),
        num_threads=4, provider="cpu")
cfg = so.OfflineTtsConfig(model=mc)
assert cfg.validate()
tts = so.OfflineTts(cfg)
t_load = time.perf_counter() - t0


def gen_cfg():
    g = so.GenerationConfig()
    if kind == "supertonic":
        g.sid = 6
        g.num_steps = 8
        g.extra["lang"] = "en"
    else:
        a, sr = sf.read(ref, dtype="float32")
        if a.ndim > 1:
            a = a[:, 0]
        g.reference_audio = a
        g.reference_sample_rate = sr
        g.num_steps = 5
    return g


lines = [l.strip() for l in open(lines_path) if l.strip()]
tts.generate("Warm up sentence.", gen_cfg())
rows = []
for i, text in enumerate(lines):
    first = []
    t0 = time.perf_counter()

    def cb(samples, progress):
        if not first:
            first.append(time.perf_counter() - t0)
        return 1

    a = tts.generate(text, gen_cfg(), cb)
    total = time.perf_counter() - t0
    dur = len(a.samples) / a.sample_rate
    rows.append((first[0] if first else total, total, dur))
    sf.write(f"out_{kind}_{i:02d}.wav", a.samples, a.sample_rate, subtype="PCM_16")
tt = np.array([r[0] for r in rows])
rt = np.array([r[1] / r[2] for r in rows])
print(json.dumps({"kind": kind, "load_s": round(t_load, 2), "first_cb_median_s": round(float(np.median(tt)), 3),
                  "first_cb_max_s": round(float(tt.max()), 3), "rtf_median": round(float(np.median(rt)), 3),
                  "rtf_total": round(sum(r[1] for r in rows) / sum(r[2] for r in rows), 3),
                  "gen_total_s": round(sum(r[1] for r in rows), 2), "audio_total_s": round(sum(r[2] for r in rows), 2)}))
for i, r in enumerate(rows):
    print(i, [round(x, 3) for x in r])
