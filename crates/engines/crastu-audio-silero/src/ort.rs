//! The Silero VAD model on ONNX Runtime, through the session registry of `crastu-ml` (feature
//! `ort`). The session is shared (one per model and options per process); each predictor
//! holds its own stream state.
//!
//! The model's signature, checked 2026-11-13 from the ONNX file itself (`silero_vad.onnx`,
//! sha256 1a153a22..., catalogue revision 066409f3 of snakers4/silero-vad: ONNX opset 16, IR 8,
//! produced by spox) and from `OnnxWrapper` in `src/silero_vad/utils_vad.py` at that revision;
//! the two agree:
//!
//! - inputs: `input` f32 `[batch, N]`, `state` f32 `[2, batch, 128]`, `sr` int64 scalar;
//! - outputs: `output` f32 `[batch, 1]` (speech probability) and `stateN` f32 `[2, batch, 128]`
//!   (the state for the next call);
//! - at 16 kHz the wrapper feeds N = 64 + 512: the last 64 samples of the previous window (zeros
//!   at the start) followed by the 512 new ones, and keeps the last 64 samples of that input as
//!   the next context; the state starts as zeros and `stateN` replaces it after each call.
//!
//! The wrapper also resets state when the sample rate or batch size changes, which cannot
//! happen here (16 kHz, one stream).

use crastu_ml::onnx::ort::value::Tensor;
use crastu_ml::onnx::{OrtFactory, OrtSession};
use crastu_ml::{Handle, ModelId, Registry, SessionOptions};

use crastu_audio::AudioError;

use crate::gate::{Predictor, SileroConfig, SileroGate, WINDOW_SAMPLES};

/// Samples of the previous window the model wants in front of each new one (16 kHz).
const CONTEXT_SAMPLES: usize = 64;
const STATE_LEN: usize = 2 * 128;
const SAMPLE_RATE: i64 = 16_000;

/// A [`Predictor`] over a Silero VAD session.
pub struct OrtPredictor {
    session: Handle<OrtFactory>,
    state: Vec<f32>,
    context: Vec<f32>,
}

impl std::fmt::Debug for OrtPredictor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OrtPredictor")
            .field("model", self.session.model())
            .finish_non_exhaustive()
    }
}

impl OrtPredictor {
    pub fn new(session: Handle<OrtFactory>) -> Self {
        OrtPredictor {
            session,
            state: vec![0.0; STATE_LEN],
            context: vec![0.0; CONTEXT_SAMPLES],
        }
    }
}

fn run(session: &OrtSession, input: Vec<f32>, state: Vec<f32>) -> Result<(f32, Vec<f32>), String> {
    let e = |what: &str, err: &dyn std::fmt::Display| format!("{what}: {err}");
    let n = input.len();
    let input = Tensor::from_array(([1usize, n], input)).map_err(|x| e("input", &x))?;
    let state = Tensor::from_array(([2usize, 1, 128], state)).map_err(|x| e("state", &x))?;
    let sr = Tensor::from_array(([0usize; 0], vec![SAMPLE_RATE])).map_err(|x| e("sr", &x))?;
    let mut guard = session.lock();
    let outputs = guard
        .run(crastu_ml::onnx::ort::inputs!["input" => input, "state" => state, "sr" => sr])
        .map_err(|x| e("run", &x))?;
    let (_, prob) = outputs["output"]
        .try_extract_tensor::<f32>()
        .map_err(|x| e("output", &x))?;
    let (_, next) = outputs["stateN"]
        .try_extract_tensor::<f32>()
        .map_err(|x| e("stateN", &x))?;
    let prob = *prob.first().ok_or("the model returned no probability")?;
    if next.len() != STATE_LEN {
        return Err(format!(
            "stateN has {} values, expected {STATE_LEN}",
            next.len()
        ));
    }
    Ok((prob, next.to_vec()))
}

impl Predictor for OrtPredictor {
    fn predict(&mut self, window: &[f32]) -> Result<f32, String> {
        if window.len() != WINDOW_SAMPLES {
            return Err(format!(
                "the window has {} samples, expected {WINDOW_SAMPLES}",
                window.len()
            ));
        }
        let mut input = Vec::with_capacity(CONTEXT_SAMPLES + WINDOW_SAMPLES);
        input.extend_from_slice(&self.context);
        input.extend_from_slice(window);
        // The context is the end of this input, whether or not the run succeeds: the stream
        // moved on either way.
        self.context.clear();
        self.context
            .extend_from_slice(&input[input.len() - CONTEXT_SAMPLES..]);
        let (prob, next) = run(&self.session, input, self.state.clone())?;
        self.state = next;
        Ok(prob)
    }

    fn reset(&mut self) {
        self.state.iter_mut().for_each(|v| *v = 0.0);
        self.context.iter_mut().for_each(|v| *v = 0.0);
    }
}

/// A gate on the Silero model. The model file must be registered on the registry's factory
/// first (`OrtFactory::register_model`); the session is shared with every other holder of the
/// same model and options.
pub fn silero_gate(
    registry: &Registry<OrtFactory>,
    model: &ModelId,
    options: SessionOptions,
    cfg: SileroConfig,
) -> Result<SileroGate<OrtPredictor>, AudioError> {
    let handle = registry
        .load(model, options)
        .map_err(|e| AudioError::Model(e.to_string()))?;
    Ok(SileroGate::new(OrtPredictor::new(handle), cfg))
}

#[cfg(test)]
#[path = "ort_tests.rs"]
mod tests;
