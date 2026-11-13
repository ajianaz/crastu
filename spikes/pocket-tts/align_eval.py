"""Throwaway: round-trip WER (Parakeet TDT 110m int8 via sherpa-onnx) and word-timing estimates vs Parakeet.

usage: align_eval.py LINES ASR_DIR NAME=PATTERN [NAME=PATTERN ...]   (PATTERN has {i:02d})
"""
import sys
import re
import json
import difflib
import wave

import numpy as np
import scipy.signal as sg
import sherpa_onnx as so

lines_path, asr_dir = sys.argv[1:3]
sets = dict(a.split("=", 1) for a in sys.argv[3:])
lines = [l.strip() for l in open(lines_path) if l.strip()]

rec = so.OfflineRecognizer.from_transducer(
    encoder=f"{asr_dir}/encoder.int8.onnx", decoder=f"{asr_dir}/decoder.int8.onnx",
    joiner=f"{asr_dir}/joiner.int8.onnx", tokens=f"{asr_dir}/tokens.txt",
    num_threads=4, model_type="nemo_transducer")


def read(p):
    w = wave.open(p)
    sr = w.getframerate()
    x = np.frombuffer(w.readframes(-1), dtype=np.int16).astype(np.float32) / 32768
    return x, sr


def norm_words(t):
    return re.sub(r"[^a-z0-9' ]", " ", t.lower().replace("-", " ")).split()


def asr_words(x, sr):
    s = rec.create_stream()
    s.accept_waveform(sr, x)
    rec.decode_stream(s)
    r = s.result
    words, starts = [], []
    for tok, ts in zip(r.tokens, r.timestamps):
        if tok.startswith(" ") or not words:
            words.append(tok.lstrip(" "))
            starts.append(ts)
        else:
            words[-1] += tok
    return words, np.array(starts), r.text


def syl(w):
    return max(1, len(re.findall(r"[aeiouy]+", re.sub(r"e$", "", w)) or [1]))


def wer(ref, hyp):
    sm = difflib.SequenceMatcher(None, ref, hyp)
    same = sum(b.size for b in sm.get_matching_blocks())
    return 1 - same / max(1, len(ref))  # rough: share of reference words not matched in order


def speech_span(x, sr):
    e = np.convolve(x ** 2, np.ones(int(0.02 * sr)) / int(0.02 * sr), "same")
    on = np.where(e > 0.02 * e.max())[0]
    return on[0] / sr, on[-1] / sr


def nuclei(x, sr):
    b, a = sg.butter(3, [300 / (sr / 2), 3000 / (sr / 2)], "band")
    y = np.abs(sg.filtfilt(b, a, x))
    b2, a2 = sg.butter(2, 8 / (sr / 2))
    env = sg.filtfilt(b2, a2, y)
    pk, _ = sg.find_peaks(env, distance=int(0.09 * sr), prominence=0.08 * env.max())
    return pk / sr


def est_prop(words, weights, t0, t1):
    w = np.array(weights, dtype=float)
    c = np.concatenate([[0], np.cumsum(w)]) / w.sum()
    return t0 + c[:-1] * (t1 - t0)


def est_nuclei(words, nuc, t0):
    sy = [syl(w) for w in words]
    first = np.concatenate([[0], np.cumsum(sy)[:-1]])
    n_syl = sum(sy)
    if len(nuc) < 2:
        return None
    idx = np.round(first * (len(nuc) - 1) / max(1, n_syl - 1)).astype(int)
    return nuc[np.clip(idx, 0, len(nuc) - 1)]


for name, pat in sets.items():
    errs = {"chars": [], "syllables": [], "nuclei": []}
    wers, ratio = [], []
    for i, text in enumerate(lines):
        x, sr = read(pat.format(i=i))
        ref = norm_words(text)
        hw, hs, htext = asr_words(x, sr)
        hyp = [h.lower() for h in hw]
        hyp = norm_words(" ".join(hyp))
        wers.append(wer(ref, hyp))
        if len(hw) != len(ref):
            sm = difflib.SequenceMatcher(None, ref, [re.sub(r"[^a-z0-9']", "", h.lower()) for h in hw])
            pairs = [(a + k, b + k) for a, b, n in sm.get_matching_blocks() for k in range(n)]
        else:
            pairs = list(zip(range(len(ref)), range(len(ref))))
        t0, t1 = speech_span(x, sr)
        nuc = nuclei(x, sr)
        ratio.append((len(nuc), sum(syl(w) for w in ref)))
        e_c = est_prop(ref, [len(w) + 1 for w in ref], t0, t1)
        e_s = est_prop(ref, [syl(w) for w in ref], t0, t1)
        e_n = est_nuclei(ref, nuc, t0)
        for ri, hi in pairs:
            ref_t = hs[hi]
            errs["chars"].append(e_c[ri] - ref_t)
            errs["syllables"].append(e_s[ri] - ref_t)
            if e_n is not None:
                errs["nuclei"].append(e_n[ri] - ref_t)
    out = {"set": name, "words_unmatched_share": round(float(np.mean(wers)), 3),
           "nuclei_vs_syllables": f"{sum(a for a,_ in ratio)}/{sum(b for _,b in ratio)}"}
    for k, v in errs.items():
        v = np.array(v) * 1000
        out[k] = {"n": len(v), "median_signed_ms": int(np.median(v)), "median_abs_ms": int(np.median(np.abs(v))),
                  "p90_abs_ms": int(np.percentile(np.abs(v), 90)),
                  "within_100ms": round(float(np.mean(np.abs(v) <= 100)), 2)}
    print(json.dumps(out))
