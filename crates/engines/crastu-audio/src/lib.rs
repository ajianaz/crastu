//! Audio capture, playback, resampling and voice activity detection.
//!
//! Slice 1: the capture seam. An [`AudioSource`] hands out mono `f32` samples at the device's
//! own sample rate; a [`Converter`] turns them into fixed 20 ms frames at 16 kHz with a
//! [`Level`] and a clipping flag per frame; a [`VoiceGate`] says whether a frame holds speech.
//! The sources are a generator ([`SyntheticSource`]) and, behind the `mic` cargo feature, the
//! microphone; the WAV file source lives in `crastu-audio-wav`. Everything but the microphone
//! is safe code that needs no hardware, so it is tested without any, and it compiles for iOS
//! and Android.
//!
//! Permission handling stays out of this crate (it is an engine, and the OS-specific query lives
//! in `crastu-os-perms`); [`AudioError::PermissionDenied`] and [`AudioError::PermissionUnknown`]
//! carry the same words, for the caller to map.

mod convert;
mod error;
mod gate;
mod level;
#[cfg(feature = "mic")]
mod mic;
mod resample;
mod ring;
mod source;
mod synthetic;

pub use convert::{Converter, FRAME_MS, FRAME_SAMPLES, Frame, TARGET_RATE, downmix_interleaved};
pub use error::AudioError;
pub use gate::{EnergyGate, GateConfig, GateEvent, VoiceGate};
pub use level::{CLIP_THRESHOLD, Level, SILENCE_DB, count_clipped, level_of};
#[cfg(feature = "mic")]
pub use mic::{MicSource, MicStats, input_device_names};
pub use resample::{MAX_RATE, Resampler, validate_rate};
pub use ring::{Consumer, Producer, ring_buffer};
pub use source::{AudioSource, ReadResult};
pub use synthetic::{SyntheticSource, Waveform};
