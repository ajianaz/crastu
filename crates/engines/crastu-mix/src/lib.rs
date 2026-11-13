//! One mixer graph for preview and export (issue #33).
//!
//! A [`Timeline`] of voice and music [`Clip`]s becomes events ([`Timeline::events`]); the
//! [`Mixer`] pulls [`PcmSource`]s through clip gain and fades, a voice bus that ducks the music
//! bus ([`Ducker`]), a bus sum, a master gain and a true-peak [`Limiter`]. Time is an integer
//! sample position and every stage is either a pure function of that position or a per-sample
//! causal filter, so the output is bit-identical for any block size, and `Mixer::process` (the
//! device callback) and `Mixer::bounce` (export) are the same code. [`LoudnessMeter`] measures
//! BS.1770 integrated loudness and [`gain_to_target`] turns a measurement into a master gain.
//!
//! Safe code, std only, no hardware: it compiles for iOS and Android. The device callback itself
//! is the caller's; it calls `process` with its buffer.

mod curve;
mod duck;
mod limiter;
mod mixer;
mod source;
mod timeline;

pub use crastu_loudness::{
    BROADCAST_TARGET_LUFS, LoudnessMeter, SOCIAL_TARGET_LUFS, gain_to_target, gain_to_target_db,
    measure_lufs,
};
pub use curve::{Automation, db_to_gain, smoothstep};
pub use duck::Ducker;
pub use limiter::{Limiter, true_peak};
pub use mixer::{MixConfig, Mixer};
pub use source::{MemorySource, PcmCache, PcmKey, PcmSource};
pub use timeline::{Clip, Event, EventKind, MIN_FADE, Timeline, Track};

use std::fmt;

/// Why a [`Mixer`] could not be built.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MixError {
    /// The sample rate is zero.
    SampleRate,
    /// A source is neither mono nor stereo.
    Channels,
    /// A clip names a source that does not exist.
    UnknownSource {
        /// Index of the clip.
        clip: usize,
        /// The source index it named.
        source: usize,
    },
    /// A gain is not a finite number.
    Gain,
}

impl fmt::Display for MixError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SampleRate => write!(f, "the sample rate must be above zero"),
            Self::Channels => write!(f, "a source must be mono or stereo"),
            Self::UnknownSource { clip, source } => {
                write!(f, "clip {clip} uses source {source}, which does not exist")
            }
            Self::Gain => write!(f, "a gain is not a finite number"),
        }
    }
}

impl std::error::Error for MixError {}
