//! The Silero VAD voice gate for `crastu-audio`.
//!
//! [`SileroGate`] is a `crastu_audio::VoiceGate` on a speech probability model behind the
//! [`Predictor`] trait: it adapts the crate's 320-sample frames to the model's 512-sample
//! windows and shares the hysteresis and hangover code with the energy gate. The ONNX backend
//! (`OrtPredictor`, `silero_gate`) is behind the off-by-default feature `ort`, so the default
//! build and the mobile builds stay free of ONNX Runtime.

mod gate;
#[cfg(feature = "ort")]
mod ort;

pub use gate::{Predictor, SileroConfig, SileroGate, WINDOW_SAMPLES};
#[cfg(feature = "ort")]
pub use ort::{OrtPredictor, silero_gate};
