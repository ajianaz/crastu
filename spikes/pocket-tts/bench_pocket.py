"""Throwaway bench: Pocket TTS ONNX (KevinAHM export) through onnxruntime on CPU.

usage: bench_pocket.py MODELS_DIR LANG PRECISION VOICE_WAV LINES OUT_DIR
"""
import sys
import time
import json
from pathlib import Path

import numpy as np

models, lang, prec, voice, lines_path, out = sys.argv[1:7]
sys.path.insert(0, str(Path(models).parent))
from pocket_tts_onnx import PocketTTSOnnx  # noqa: E402

out = Path(out)
out.mkdir(parents=True, exist_ok=True)

t0 = time.perf_counter()
tts = PocketTTSOnnx(models_dir=models, language=lang, precision=prec)
t_load = time.perf_counter() - t0

if voice.endswith(".npy"):
    voice = np.load(voice)
    t_enc = t_prep = t_prep_cached = 0.0
    vname = "cached-embedding"
else:
    vname = Path(voice).name
t0 = time.perf_counter()
if isinstance(voice, str):
    emb = tts.encode_voice(voice)
    np.save(out / (Path(voice).stem + ".npy"), emb)
    t_enc = time.perf_counter() - t0
    t0 = time.perf_counter()
    tts.prepare_voice_state(voice)  # encode again + conditioning (cached after)
    t_prep = time.perf_counter() - t0
    t0 = time.perf_counter()
    tts.prepare_voice_state(voice)
    t_prep_cached = time.perf_counter() - t0
else:
    t0 = time.perf_counter()
    tts.prepare_voice_state(voice)
    t_prep = time.perf_counter() - t0
    t_prep_cached = 0.0

sr = tts.sample_rate
lines = [l.strip() for l in open(lines_path) if l.strip()]

# warm-up (not counted)
for _ in tts.stream("Warm up sentence.", voice):
    pass

rows = []
for i, text in enumerate(lines):
    t0 = time.perf_counter()
    first = None
    chunks = []
    for c in tts.stream(text, voice):
        if first is None:
            first = time.perf_counter() - t0
        chunks.append(c)
    total = time.perf_counter() - t0
    audio = np.concatenate(chunks)
    dur = len(audio) / sr
    rows.append({"i": i, "chars": len(text), "ttfa": first, "gen": total, "audio_s": dur, "rtf": total / dur})
    pcm = (np.clip(audio, -1, 1) * 32767).astype(np.int16)
    import wave
    with wave.open(str(out / f"line{i:02d}.wav"), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(sr)
        w.writeframes(pcm.tobytes())

ttfa = np.array([r["ttfa"] for r in rows])
rtf = np.array([r["rtf"] for r in rows])
tot_gen = sum(r["gen"] for r in rows)
tot_aud = sum(r["audio_s"] for r in rows)
print(json.dumps({
    "lang": lang, "precision": prec, "voice": vname,
    "load_s": round(t_load, 2), "encode_voice_s": round(t_enc, 2),
    "prep_first_s": round(t_prep, 2), "prep_cached_s": round(t_prep_cached, 4),
    "ttfa_median_s": round(float(np.median(ttfa)), 3), "ttfa_max_s": round(float(ttfa.max()), 3),
    "rtf_median": round(float(np.median(rtf)), 3), "rtf_total": round(tot_gen / tot_aud, 3),
    "gen_total_s": round(tot_gen, 2), "audio_total_s": round(tot_aud, 2),
}))
for r in rows:
    print(" ".join(f"{k}={v:.3f}" if isinstance(v, float) else f"{k}={v}" for k, v in r.items()))
