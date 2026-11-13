//! Real-model tests of the Silero gate: `#[ignore]`, driven by environment variables (see the
//! crate README):
//!
//! - `CRASTU_TEST_ORT_DYLIB`: the ONNX Runtime library;
//! - `CRASTU_TEST_ONNX_MODEL`: the catalogue's `silero_vad.onnx`;
//! - `CRASTU_TEST_SPEECH_WAV`: a 16 kHz mono 16-bit WAV with speech and pauses.
//!
//! Run them with `--ignored --test-threads=1`: a process holds one runtime for its life.

// The frames and windows are slices of a `Vec` here; arrays would only add noise.
#![allow(clippy::chunks_exact_to_as_chunks)]

use std::time::Duration;

use super::*;
use crastu_audio::{FRAME_SAMPLES, GateEvent, VoiceGate};

fn setup() -> Registry<OrtFactory> {
    let dylib = std::env::var("CRASTU_TEST_ORT_DYLIB").expect("CRASTU_TEST_ORT_DYLIB");
    let model = std::env::var("CRASTU_TEST_ONNX_MODEL").expect("CRASTU_TEST_ONNX_MODEL");
    let factory = OrtFactory::with_library(dylib);
    factory.register_model(ModelId::new("silero"), model);
    Registry::new(factory, Duration::from_secs(5))
}

fn options() -> SessionOptions {
    SessionOptions::default().with_intra_threads(1)
}

fn gate(registry: &Registry<OrtFactory>) -> SileroGate<OrtPredictor> {
    silero_gate(
        registry,
        &ModelId::new("silero"),
        options(),
        SileroConfig::default(),
    )
    .expect("the model loads")
}

/// The samples of a 16 kHz mono 16-bit PCM WAV (a `fmt ` chunk, then a `data` chunk).
fn read_wav() -> Vec<f32> {
    let path = std::env::var("CRASTU_TEST_SPEECH_WAV").expect("CRASTU_TEST_SPEECH_WAV");
    let bytes = std::fs::read(path).expect("the WAV can be read");
    assert_eq!(&bytes[..4], b"RIFF");
    let le32 = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    let le16 = |at: usize| u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap());
    let mut pos = 12;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let len = le32(pos + 4) as usize;
        if id == b"fmt " {
            let format = (le32(pos + 12), le16(pos + 10), le16(pos + 22));
            assert_eq!(format, (16_000, 1, 16), "16 kHz mono 16-bit only");
        }
        if id == b"data" {
            let end = (pos + 8 + len).min(bytes.len());
            return bytes[pos + 8..end]
                .chunks_exact(2)
                .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.0)
                .collect();
        }
        pos += 8 + len + (len & 1);
    }
    panic!("no data chunk");
}

/// How many frames the gate calls speech, and how many there were.
fn speech_frames(gate: &mut SileroGate<OrtPredictor>, samples: &[f32]) -> (usize, usize) {
    let mut speech = 0;
    let mut total = 0;
    for frame in samples.chunks_exact(FRAME_SAMPLES) {
        total += 1;
        if gate.process(frame).is_speech() {
            speech += 1;
        }
    }
    (speech, total)
}

#[test]
#[ignore = "needs CRASTU_TEST_ORT_DYLIB, CRASTU_TEST_ONNX_MODEL, CRASTU_TEST_SPEECH_WAV"]
fn speech_passes_and_silence_does_not_on_the_real_model() {
    let registry = setup();
    let samples = read_wav();

    let mut g = gate(&registry);
    let (speech, total) = speech_frames(&mut g, &samples);
    println!("clip: {speech} of {total} frames are speech");
    assert_eq!(g.predict_errors(), 0, "{:?}", g.last_error());
    assert!(speech * 10 > total, "speech frames pass: {speech}/{total}");
    assert!(
        speech * 100 < total * 95,
        "pauses are not all speech: {speech}/{total}"
    );

    // Digital silence, and very quiet noise (a fixed linear congruential generator), do not
    // open the gate.
    let mut g = gate(&registry);
    let zeros = vec![0.0f32; 16_000 * 5];
    assert_eq!(speech_frames(&mut g, &zeros).0, 0, "digital silence");
    let mut x = 12345u32;
    let quiet: Vec<f32> = (0..16_000 * 5)
        .map(|_| {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            ((x >> 8) as f32 / 8_388_608.0 - 1.0) * 0.002
        })
        .collect();
    let mut g = gate(&registry);
    assert_eq!(speech_frames(&mut g, &quiet).0, 0, "quiet noise");
}

#[test]
#[ignore = "needs CRASTU_TEST_ORT_DYLIB, CRASTU_TEST_ONNX_MODEL, CRASTU_TEST_SPEECH_WAV"]
fn state_is_carried_across_frames_and_reset_restarts_the_stream() {
    let registry = setup();
    let samples = read_wav();
    let run = |g: &mut SileroGate<OrtPredictor>| -> Vec<GateEvent> {
        samples
            .chunks_exact(FRAME_SAMPLES)
            .take(400)
            .map(|f| g.process(f))
            .collect()
    };
    let mut g = gate(&registry);
    let first = run(&mut g);
    g.reset();
    let second = run(&mut g);
    assert_eq!(first, second, "a reset gate answers like a new one");
    // Two gates share one session but not their state.
    let mut other = gate(&registry);
    assert_eq!(run(&mut other), first);
    assert_eq!(registry.snapshot().session_count(), 1);
    assert_eq!(registry.snapshot().total_handles(), 2);
}

/// Prints the first probabilities of the clip, to compare with a reference run of the same model
/// in Python (`onnxruntime`, the wrapper's logic of `utils_vad.py`).
#[test]
#[ignore = "needs CRASTU_TEST_ORT_DYLIB, CRASTU_TEST_ONNX_MODEL, CRASTU_TEST_SPEECH_WAV"]
fn print_probabilities_for_comparison() {
    let registry = setup();
    let samples = read_wav();
    let handle = registry.load(&ModelId::new("silero"), options()).unwrap();
    let mut p = OrtPredictor::new(handle);
    let probs: Vec<String> = samples
        .chunks_exact(WINDOW_SAMPLES)
        .take(80)
        .map(|w| format!("{:.5}", p.predict(w).unwrap()))
        .collect();
    println!("PROBS {}", probs.join(" "));
}

/// A fixed signal, built the same way in the Python reference run (`onnxruntime` 1.31.1 and the
/// wrapper's logic of `utils_vad.py`, 2026-11-13): a 150 Hz tone with a slow wobble plus two
/// weaker tones. Its probabilities depend on the carried state and context, so a predictor that
/// drops either gives other numbers (the reference without state or without context differs by
/// 0.04 to 0.4 from the sixth window on).
#[test]
#[ignore = "needs CRASTU_TEST_ORT_DYLIB and CRASTU_TEST_ONNX_MODEL"]
fn probabilities_match_the_reference_run_on_a_fixed_signal() {
    let registry = setup();
    let tau = std::f64::consts::TAU;
    let signal: Vec<f32> = (0..16_000)
        .map(|i| {
            let t = f64::from(i) / 16_000.0;
            (0.3 * (tau * 150.0 * t).sin() * (1.0 + 0.5 * (tau * 3.0 * t).sin())
                + 0.1 * (tau * 450.0 * t).sin()
                + 0.05 * (tau * 1200.0 * t).sin()) as f32
        })
        .collect();
    let reference = [
        0.22352, 0.13038, 0.05778, 0.02136, 0.01353, 0.00720, 0.00566, 0.00540, 0.00441, 0.00763,
        0.00440, 0.00405,
    ];
    let handle = registry.load(&ModelId::new("silero"), options()).unwrap();
    let mut p = OrtPredictor::new(handle);
    for (i, want) in reference.iter().enumerate() {
        let got = p
            .predict(&signal[i * WINDOW_SAMPLES..(i + 1) * WINDOW_SAMPLES])
            .unwrap();
        assert!(
            (got - want).abs() < 2e-3,
            "window {i}: {got} against {want}"
        );
    }
    // A reset stream starts again from the same state.
    p.reset();
    let again = p.predict(&signal[..WINDOW_SAMPLES]).unwrap();
    assert!((again - reference[0]).abs() < 2e-3, "{again}");
}
