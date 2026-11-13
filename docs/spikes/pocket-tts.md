# Spike: Pocket TTS as a built-in Synthesizer

Issue #27. Run on 2026-11-13. Code: [`spikes/pocket-tts`](../../spikes/pocket-tts) (three throwaway Python scripts, not part of the workspace). Every external fact below was read from its primary source on 2026-11-13 and carries the link; where two sources disagree the note says so.

## Question

Open question 3 (Supertonic 3's upstream is archived) and open question 4 (no commercially clean on-device cloning model) block P2. Kyutai's Pocket TTS (about 100 M parameters) runs on a CPU through ONNX, streams, and can take a voice from a short clip. Can it be a second built-in engine behind the `Synthesizer` seam, and can it be the answer to cloning?

## Result in short

- **Licences hold for the presets.** The base weights are CC-BY-4.0 (commercial use allowed, attribution required), the ONNX export we would use is CC-BY-4.0 too, and the variant of the weights with the voice-cloning encoder zeroed out is not gated. Built-in presets can ship, with three conditions: ship only the voices whose source is CC0 or CC BY (two of the default voices come from non-commercial datasets), pin the revisions, and carry the attribution text below.
- **Cloning is not clean enough to ship in the bundle.** The weights that can clone are the same licence, but their repository is gated (a click-through "Prohibited use" prompt that names "voice impersonation or cloning without explicit and lawful consent"), and ARCHITECTURE section 8 denies gated models for shipping. It can be an opt-in download behind a consent dialog, labelled Conditional. With the community export's encoder, the whole process peaked at 1.7 GB with a 10 s clip (about 0.7 GB of that is the loaded models) and at 10.8 GB (peak memory footprint) with a 30 s clip on this 8 GB Mac, so the 10 to 30 s clip the issue asked for does not work with it as exported.
- **Languages: no Indonesian.** Pocket TTS covers English, French, German, Portuguese, Italian and Spanish (plus Dutch files in the weights repo). Supertonic 3 lists 31 languages including Indonesian (`id`). Pocket TTS is a hedge for open question 3 in six languages, not a replacement.
- **Speed (M2, shared and busy machine, see Method):** time to first audio about 0.15 s median (streaming wrapper), real-time factor 0.22 to 0.36 (median per line; 0.26 to 0.41 over all ten lines) with a cloned or cached voice; Supertonic 3 through sherpa-onnx on the same 10 lines: RTF about 0.29, and it delivers a whole line at once (first audio about 1.4 s median).
- **Captions:** estimating word times from text and audio lands a median 100 to 125 ms from Parakeet's word starts, 90th percentile 300 to 390 ms. Use an aligner for captions.

## 1. Licences (step 1: do these hold?)

Checked with the Hugging Face API and raw files, and the GitHub API, on 2026-11-13.

| Piece | Source | Licence id | Commercial | Notes |
|---|---|---|---|---|
| Base weights and the weights that can clone | [kyutai/pocket-tts](https://huggingface.co/kyutai/pocket-tts) (revision `db04f0e6`, API `https://huggingface.co/api/models/kyutai/pocket-tts`) | CC-BY-4.0 | Yes, with attribution | `gated: "auto"`: a login and a click-through with the prohibited-use text. The card cannot be read without a login. |
| Same weights, voice cloning removed | [kyutai/pocket-tts-without-voice-cloning](https://huggingface.co/kyutai/pocket-tts-without-voice-cloning) (revision `a7056f50`) | CC-BY-4.0 | Yes, with attribution | `gated: false`. The repository's own script `remove_voice_cloning_and_push.py` zeroes every tensor whose name starts with `mimi.encoder`. Same README, same prohibited-use text. |
| Kyutai's code | [kyutai-labs/pocket-tts](https://github.com/kyutai-labs/pocket-tts) | MIT (GitHub API `license.spdx_id`) | Yes | Concepts only for us; we do not need the code. Training code was added in August 2026 (the card). |
| ONNX export we would use | [KevinAHM/pocket-tts-onnx](https://huggingface.co/KevinAHM/pocket-tts-onnx) (revision `58a6d00c`, last modified 2026-04-21) | Models CC-BY-4.0, wrapper code Apache-2.0 (the card's "License" section; `onnx/LICENSE` is the CC BY 4.0 text, byte for byte the same as the one inside the sherpa-onnx archive) | Yes, with attribution | The repository has no Apache-2.0 file for the code; we would not use the wrapper. Weights derive from the gated repository, so the export includes the real cloning encoder (`mimi_encoder.onnx`). |
| sherpa-onnx Pocket TTS archive | [`sherpa-onnx-pocket-tts-int8-2026-01-26.tar.bz2`](https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/sherpa-onnx-pocket-tts-int8-2026-01-26.tar.bz2) (98 MB, sha256 `2f3b8882...67cb`) | Archive `LICENSE` is CC BY 4.0 | **Sources disagree** | The archive's `README.md` says "It is for non-commercial". Its `LICENSE` and Kyutai's card say CC BY 4.0. The README points at `KevinAHM/pocket-tts-onnx-export`, which returns 404 today. |
| Predefined voices | [kyutai/tts-voices](https://huggingface.co/kyutai/tts-voices) README (not gated) | Per voice, see below | Mixed | The embeddings in `kyutai/pocket-tts` are computed from these clips. |

**Licence history.** The weights repository was MIT when created (2026-01-06, README at commit `09f82648`) and became CC-BY-4.0 on 2026-01-08 (commit `b5dcb3ad`; the history of `kyutai/pocket-tts-without-voice-cloning`). Pin a revision, not `main`. A possible explanation for the sherpa-onnx README's "non-commercial" is an earlier or mistaken reading; this is a guess, so ask the sherpa-onnx maintainers or Kyutai before relying on the archive.

**The prohibited-use text and CC BY 4.0.** Every Kyutai repository and the ONNX card carry the same "Prohibited use" paragraph (voice cloning without explicit and lawful consent, disinformation, and so on). It sits in the README and in the gate prompt, not in the licence file. CC BY 4.0 section 2(a)(5)(B) ("No downstream restrictions", in the `onnx/LICENSE` text) says a licensee may not impose additional or different terms on the licensed material if that restricts what a recipient may do with it. So whether the paragraph can bind us or our users, and whether our own consent dialog for cloning counts as such a term, is a legal question this note does not settle; counsel should read it before cloning ships. The paragraph is the policy we would follow anyway (consent for cloning), so presets are not affected.

### Predefined voices

From the voice list in the [model card](https://huggingface.co/kyutai/pocket-tts-without-voice-cloning) and the dataset sections of the [voices README](https://huggingface.co/kyutai/tts-voices/raw/main/README.md), checked 2026-11-13:

| Voice (language) | Source folder | Licence | Ship in the bundle |
|---|---|---|---|
| alba | alba-mackenna | CC BY 4.0 | Yes, with credit |
| anna, azelma, charles, eponine, eve, fantine, george, jane, mary, michael, paul, vera (en) | vctk | CC BY 4.0 | Yes, with credit to the VCTK corpus |
| bill_boerst, caro_davy, peter_yearsley, stuart_bell (en) | voice-zero | CC0 | Yes |
| javert, marius (en) | voice-donations | CC0 | Yes |
| estelle (fr) | unmute-prod-website | "our own recordings ... CC0" (the same README says three files in that folder are not); the file used is `developpeuse-3.wav`, not one of the three named | Yes, once the file is confirmed |
| **cosette** (en) | expresso | **CC BY-NC 4.0, "Non-commercial use only"** | **No** |
| **jean** (en) | ears | **CC BY-NC 4.0, "Non-commercial use only"** | **No** |
| giovanni (it), lola (es), juergen (de), rafael (pt) | files on the branch `add_lang_not_documented` of `kyutai/pocket-tts` | Not documented in the voices README | **No, until Kyutai documents them** |

The embedding of a voice derived from a non-commercial clip is a derivative of it, so the files `embeddings*/cosette.safetensors` and `embeddings*/jean.safetensors` must not ship either. The embeddings are in the ungated repository too, so the gate does not stop anyone from fetching them.

### Catalogue entries (proposed, format of `crates/models/crastu-models/catalogue.json`)

The files of the English 6-layer bundle `english_2026-04` of the ONNX export, revision `58a6d00c`. sha256 from the files downloaded on 2026-11-13; sizes in bytes.

| File | Size | sha256 | Needed for |
|---|---|---|---|
| `flow_lm_main_int8.onnx` | 76,341,079 | `f9bd8106b79a0192c1c43399ab938fb24900a95c1c599870d75a884e99000116` | presets |
| `flow_lm_flow_int8.onnx` | 9,962,530 | `3dd781ee5abee9e195320bf0106bebd6372a852b3b36352524ee78b40554635d` | presets |
| `mimi_decoder_int8.onnx` | 22,684,077 | `3630450a3297a101792a6ac66619ebc70ab916b265e6220c2afaef8b1673f925` | presets |
| `text_conditioner.onnx` (fp32) | 16,388,344 | `4ecee995fb69f85c7a7493d11f7b5ee15d9950facc7ab3f5c9c49ef1e03847bb` | presets |
| `tokenizer.model`, `bundle.json`, `bos_before_voice.npy` | 59,339 / 24,381 / 4,224 | `d461765a...c3f6`, `bab64315...28b3f`, `f46edf4f...80b1` | presets |
| `mimi_encoder.onnx` (fp32) | 39,768,446 | `853e2ca623b8782d94c3745ec6133bfdff7ce33d9b11128bd29ea03f28d76e3d` | cloning only |

Presets only: about 125 MB. With the encoder: about 165 MB. The licence id is `CC-BY-4.0`. The commercial flag is `Allowed` for the presets entry and `Conditional` for the cloning entry, with a note along the lines of "Voice cloning requires the consent of the person whose voice is used (see the model's prohibited-use terms); Crastu asks before it saves a voice." Attribution text, to show in Settings and in the licences file (CC BY 4.0 asks for credit, the licence link, and a note of changes):

> Pocket TTS by Kyutai (https://huggingface.co/kyutai/pocket-tts), licensed under CC BY 4.0 (https://creativecommons.org/licenses/by/4.0/). Converted to ONNX and quantised to int8 by KevinAHM (https://huggingface.co/KevinAHM/pocket-tts-onnx). Voices: Alba MacKenna (CC BY 4.0), the VCTK corpus (CC BY 4.0), Voice-Zero and the Unmute's own recordings (CC0) (CC0).

## 2. Method

| Item | Value |
|---|---|
| Machine | Mac mini, Apple M2 (8 cores), 8 GB, macOS 26.5.1 |
| Load while measuring | **Busy.** `uptime` showed a load average of 8 to 21 on 8 cores (other builds running), so every time below is an upper bound for an idle machine, and repeated runs differ by up to 1.5x (shown as ranges) |
| Runtime A (Pocket, streaming, cloning) | Python 3.12.9 and `onnxruntime` 1.22.1 (CPU provider) running the KevinAHM wrapper `pocket_tts_onnx.py` (revision `58a6d00c`), int8 models, `lsd_steps=1`, temperature 0.7, 4 intra-op threads (the wrapper's default) |
| Runtime B (Pocket and Supertonic 3 through sherpa-onnx) | `sherpa-onnx` 2.0.1 Python wheel (CPU, 4 threads) with the release archives `sherpa-onnx-pocket-tts-int8-2026-01-26` (sha256 `2f3b8882...67cb`) and `sherpa-onnx-supertonic-3-tts-int8-2026-05-11` (sha256 `82fa96f9...c432`; Supertonic 3 is OpenRAIL-M, see decision 8). Pocket: 5 steps; Supertonic: speaker 6, 8 steps, English |
| Not done | **The `ort` crate in `crastu-ml` was not used.** The Python runtime is the same ONNX Runtime engine but the flow loop, state handling and sentence splitting are the wrapper's Python; a Rust port is the next slice (see below). Runtime B is the same C++ code the official Rust sherpa-onnx API calls |
| Voice for cloning | Public clips from `kyutai/tts-voices/alba-mackenna` (CC BY 4.0): `casual.wav` (10.0 s, 48 kHz) alone; two clips joined with ffmpeg for 20 s; three joined for 30 s. The wrapper loads built-in voices from the gated repository, which was not accessed, so no built-in voice was measured: every Pocket number below uses a cloned voice embedding (cached or computed from the clip). The wrapper also recomputes the voice conditioning on each `stream()` call, so first-audio and RTF include it (an app that caches the state would do slightly better) |
| Lines | 10 English lines (`spikes/pocket-tts/lines.txt`): 53 to 83 characters, numbers, a list of steps, a question |
| Timing | `time.perf_counter()` around each line after one warm-up line. Time to first audio (TTFA) for runtime A is the first chunk yielded by `stream()` (adaptive chunking, first chunk 2 frames = 160 ms of audio). Sherpa's callback fired only at the end of a line for both models (the first callback time is about equal to the total for Supertonic and within 0.1 s of it for Pocket), so for runtime B the "first audio" column is the whole line, not a streamed chunk |
| Memory | `/usr/bin/time -l` on the whole process (maximum resident set size and peak memory footprint), Python and numpy included (about 40 to 90 MB) |

## 3. Results

### Speed and memory, 10 lines

| Run | TTFA median (max) | RTF median / total | Audio for 10 lines | Peak RSS | Peak footprint |
|---|---|---|---|---|---|
| A, int8, voice from a 10 s clip, two runs | 0.14 s (0.30 to 0.63) | 0.22 / 0.26 to 0.29 | 36.7 to 40.2 s | 1.29 to 1.81 GB | 1.67 to 1.70 GB |
| A, int8, cached voice embedding, three runs | 0.16 to 0.20 s (0.31 to 0.49) | 0.26 to 0.36 / 0.27 to 0.41 | 37.8 to 39.6 s | 0.74 to 1.10 GB | 0.68 GB (one run) |
| A, fp32, cached voice embedding | 0.26 s (0.75) | 0.42 / 0.47 | 37.2 s | 1.28 GB | 1.12 GB |
| A, int8, voice from a 20 s clip | 0.20 s (0.31) | 0.24 / 0.24 | 45.9 s | 1.70 GB | **5.19 GB** |
| A, int8, voice from a 30 s clip | 0.45 s (1.46) | 0.47 / 0.63 | 36.1 s | 2.12 GB | **10.79 GB** |
| B, Pocket (sherpa-onnx), 10 s clip | 0.52 s (0.66), whole line | 0.20 / 0.20 | 29.9 s | 0.69 GB | 0.66 GB |
| B, Supertonic 3 (sherpa-onnx), two runs | 1.37 s (2.4 to 2.6), whole line | 0.26 to 0.28 / 0.29 to 0.30 | 48.3 s | 0.37 to 0.45 GB | 0.43 GB (one run) |

Reading it:

- Streaming works as promised for runtime A: the first 160 ms chunk is ready after about 0.15 to 0.2 s, and generation runs 3 to 5 times faster than real time. Kyutai's own claims (about 200 ms to the first chunk, about 6x real time on a MacBook Air M4, [model card](https://huggingface.co/kyutai/pocket-tts-without-voice-cloning)) are in the same range; our numbers are on an M2 that was busy, with the int8 export, not a like-for-like check of theirs.
- Supertonic 3 is about as fast overall (RTF about 0.29) but it speaks a sentence at a time through sherpa-onnx, so the wait for the first sound is the whole sentence (1 to 2.6 s here). For sentence-by-sentence playback in the voice studio either works; for a "speak this" button, Pocket's 0.15 s is the better feel.
- The 20 s and 30 s rows are the cloning encoder: encoding took 0.77 to 0.95 s for 10 s, 3.0 s for 20 s and 8.5 s for 30 s, and the **peak memory footprint reached 10.8 GB on an 8 GB machine, so macOS was compressing and swapping**; the 30 s synthesis numbers (RTF 0.47 to 0.63) are contaminated by that and should not be read as the cost of a long voice prompt. The footprint grows far faster than the clip length. A longer prompt also costs a little more per generation step (the prompt sits in the attention cache); the 20 s run (RTF 0.24) shows no large effect.
- Model load: 0.3 to 0.8 s for every runtime, with the files just downloaded (likely in the page cache). Cold disk loads were not measured.
- The steady-state number to plan with is 0.7 to 1.1 GB for Pocket with a cached voice embedding (A) or 0.7 GB (B), against 0.4 GB for Supertonic 3. `approx_memory` for the registry (decision 31) is the file size, which would say about 0.13 GB for Pocket; the registry's idle expiry matters more for this engine than for Supertonic.

### Quality against Supertonic 3

**Not measured by ear.** There is no listening in a headless session, and no speaker-similarity model was run, so naturalness and how close a clone sounds are open ("needs hands-on": play the 10 WAVs of each engine side by side). What was measured is intelligibility, by round trip: each engine's 10 lines went through Parakeet TDT 110M int8 (`sherpa-onnx-nemo-parakeet_tdt_transducer_110m-en-36000-int8`, sherpa-onnx 2.0.1), and the transcript was compared with the text (lower-cased words, longest in-order match; a rough rate, not a WER).

| Engine | Reference words not matched in order |
|---|---|
| Pocket, runtime A, cloned voice | 0 of 124 words |
| Supertonic 3, runtime B | 0 of 124 |
| Pocket, runtime B | 3 of 124 (121 matched). Sherpa's Pocket also speaks faster: 29.9 s of audio for the same lines that took 37 to 40 s in runtime A |

All three read the lines clearly enough for an ASR to recover them. The differences in pace and voice are for a listening test.

### Languages

Pocket TTS: English, French, German, Portuguese, Italian, Spanish (card, `language` metadata and the "Multi-language support" bullet), with 6-layer bundles and larger `_24l` bundles ("higher quality but slower", card) of about 305 MB per int8 model file in the ONNX export, and Dutch files in the weights repository that the card does not list. **No Indonesian**, no Asian language. Supertonic 3: 31 languages including `id`, `ja`, `ko`, `hi`, `vi` ([Supertone/supertonic-3](https://huggingface.co/Supertone/supertonic-3) metadata and README table). Languages other than English were not run in this spike.

### Cloning from a clip, and what decision 7 implies

What ran: a 10 s, 20 s and 30 s clip of one voice became a speaker embedding with `mimi_encoder.onnx` (fp32), and the 10 lines were generated in that voice; the audio was read back by an ASR (above). How close the clone is to the speaker was not measured.

What the UI needs, from decision 7 ("the UI labels them with their licence") and the prohibited-use text:

1. The engine and the cloning feature are listed in Settings, Models and licences with the **Conditional** badge and the note above; the cloning model is a separate download, never part of the installer.
2. Before the first clone, and whenever a new voice is saved, a dialog says that cloning someone's voice needs their explicit and lawful consent, shows the model's prohibited-use paragraph and asks the user to confirm "I have the consent of the speaker (or it is my own voice)". The confirmation is stored with the voice (who, when, the clip's file name, never the audio in the log).
3. A saved cloned voice is labelled as cloned wherever voices are listed and in exported metadata ("Cloned voice, generated by Crastu with Pocket TTS").
4. A clip limit: 10 s with the exported encoder as it is (a 20 s clip already needed 5.2 GB here). To allow 10 to 30 s, the encoder must be run in chunks or exported again; that is a model-engineering task, not a catalogue entry.
5. The embedding (a few MB) can be kept instead of the clip; it is the same data class as the voice and needs the same care.

Crastu does not detect misuse, so this is a record of consent, not a guarantee.

### Captions: word timing from the generated audio

The issue asks whether word times can be estimated from the audio while it is generated (syllable nuclei against the text), and how far that lands from an aligner. Reference: word start times from Parakeet TDT 110M int8 (token timestamps; the model's time step is 80 ms, so the reference itself is only good to about 80 ms and is not ground truth). Words compared: Parakeet's words are paired with the text's words by position when the counts are equal, otherwise by an in-order match (121 to 124 words per engine). Estimators, all given the text and the finished audio of the line:

- chars: each word gets time in proportion to its length between the first and last sample above 2 % of the peak energy;
- syllables: the same with a vowel-group syllable count;
- nuclei: peaks of the 300 to 3000 Hz envelope (low-passed at 8 Hz, at least 90 ms apart) are matched to the text's syllables by rank.

Error of the estimated word start minus Parakeet's start, in ms (median signed, median absolute, 90th percentile absolute, share within 100 ms):

| Engine | chars | syllables | nuclei (found / text syllables) |
|---|---|---|---|
| Pocket (A) | -69, 119, 382, 44 % | -83, 126, 390, 44 % | -6, 114, 294, 45 % (124 / 167) |
| Pocket (B) | -115, 115, 294, 47 % | -111, 113, 341, 40 % | -57, 120, 361, 44 % (113 / 167) |
| Supertonic 3 | -76, 127, 297, 40 % | -94, 128, 312, 41 % | 1, 99, 295, 50 % (127 / 167) |

So the audio-based estimate removes the constant bias (median signed error near zero) but not the spread: about half the words land within 100 ms of the aligner, and one in ten is 300 ms or more off. That is fine for highlighting a word roughly as it is spoken in a preview, and not good enough for burned-in captions or lip timing (the nuclei detector also finds only 68 to 76 % of the syllables). Since the generated audio is clean and the text is known, an aligner is cheap: running Parakeet (or the planned alignment model) over a generated line costs far less than generating it, and gives the times for SRT export (P2 exit criteria). The estimate is a fallback when no aligner is installed. These numbers come from one voice, 10 English lines and a crude detector; they show the order of magnitude only.

## 4. Verdict

| Question | Answer |
|---|---|
| Can Pocket TTS ship as a built-in engine (presets)? | **Yes, conditionally.** CC-BY-4.0 weights and export, no gate on the no-cloning weights, credit required. Conditions: only CC0 and CC BY voices (no cosette, jean, and none of giovanni, lola, juergen, rafael until documented), pinned revisions, attribution in the app, the sherpa-onnx README's "non-commercial" line cleared with its maintainers, and the prohibited-use paragraph read by counsel. Languages: en, fr, de, pt, it, es only. |
| Can it ship for cloning? | **Not in the bundle.** Opt-in download, Conditional badge, consent dialog and labels (above). The gated repository fails the "no gated models" rule in ARCHITECTURE section 8; the ungated export carries the same weights, which is a legal question, not a technical one. The exported encoder needs a memory fix for 10 to 30 s clips. |
| Does it replace Supertonic 3? | No: Supertonic covers Indonesian and 30 other languages; Pocket has a cleaner licence (CC-BY against OpenRAIL-M, which needs its use restrictions passed on) and real streaming. Keep both behind the seam. |
| Does it answer open question 3 (Supertonic archived)? | Partly: it is a second engine with a clean licence for six languages. Supertonic's own pin and redistribution check is still open. |
| Does it answer open question 4 (clean cloning model)? | Partly: licence clean but conditional, the consent flow is ours to build, and quality was not judged. |

## 5. How it would plug in

- Seam: `crastu-tts`'s `Synthesizer` capabilities (presets, languages, streaming, cloning). Pocket TTS declares `streaming: true`, `cloning: true` (only when the encoder file is installed), languages en, fr, de, pt, it, es.
- Runtime: two options, both through the shared ONNX Runtime of decision 28. (1) sherpa-onnx's Pocket TTS (already in the stack, official Rust API, measured here as RTF 0.20, but its callback delivers the line at the end, so it does not stream; it takes a reference WAV directly). (2) A Rust port of the wrapper over `OrtFactory` (decision 36): five sessions (text conditioner, flow LM main, flow network, Mimi decoder, optional Mimi encoder), a state manifest per bundle, the flow loop and sentence splitting; this is where the 0.15 s first audio comes from. The port is a slice of its own; the wrapper is Apache-2.0 per its card but has no licence file, so write it from the model, not from the wrapper's code.
- Registry: each of the five files is a session in `crastu-ml`; with the idle expiry of decision 31 the engine's 0.7 to 1.1 GB is released when it is not used.
- Presets: the ungated no-cloning repository holds the same embedding files (`embeddings/`, `embeddings_v2/`, `embeddings_v3/` and per-language folders), so the permissive voices could ship as precomputed embeddings from it, and never from the gated one at runtime (the wrapper fetches voices from `kyutai/pocket-tts`; ours must not). **Not tried:** which embedding set pairs with the `english_2026-04` ONNX bundle (the weights repository added newer English models in October 2026 and recomputed voice states for them, and voice states are model-specific); the first implementation slice must check this, or compute the embeddings itself from the CC0 and CC BY clips with the encoder.

## 6. Gaps

- No listening test and no speaker-similarity score; naturalness, accent and clone quality against Supertonic 3 are open (needs hands-on).
- Languages other than English, the 24-layer bundles and the int8 encoder were not run. Presets were measured with a cloned embedding, not a built-in one.
- The numbers come from a busy machine and one to three runs each; idle-machine numbers will be better. No Windows or CPU-only Linux numbers.
- Core ML and the GPU were not tried (Kyutai's card says it saw no GPU speed-up for batch size 1; not checked here).
- The Rust path (`ort` through `crastu-ml`) was not run.
- Built-in voice embeddings from the ungated repository against the April ONNX bundle: not tried (see section 5). The pinned weights revision (`a7056f50`, October) and the ONNX bundle (`english_2026-04`, April) are not known to match.
- The consent dialog and the clip limit are design notes, not tested.

## Sources and dates (all checked 2026-11-13)

- Kyutai weights: https://huggingface.co/kyutai/pocket-tts (API JSON; README needs login), https://huggingface.co/kyutai/pocket-tts-without-voice-cloning (README, `remove_voice_cloning_and_push.py`, commit history)
- Voices: https://huggingface.co/kyutai/tts-voices (README)
- Code: https://github.com/kyutai-labs/pocket-tts (LICENSE, GitHub API licence field)
- ONNX export: https://huggingface.co/KevinAHM/pocket-tts-onnx (README, `onnx/LICENSE`, file listing with sizes, commit history)
- sherpa-onnx: https://github.com/k2-fsa/sherpa-onnx/releases/tag/tts-models (archives, README and LICENSE inside), https://github.com/k2-fsa/sherpa-onnx/tree/master/python-api-examples (`pocket-tts.py`, `supertonic-tts.py`)
- Supertonic 3: https://huggingface.co/Supertone/supertonic-3 (metadata, README language table, licence section)
- Paper: https://arxiv.org/abs/2509.06926 (Continuous Audio Language Models; abstract only was read)
