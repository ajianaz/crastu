//! Shared domain types: ids, timecode, audio buffers, asset references, errors.
//!
//! Everything here is platform neutral and has no I/O.

mod asset;
mod audio;
mod error;
mod id;
mod time;

pub use asset::{AssetKind, AssetRef};
pub use audio::AudioBuf;
pub use error::{Error, Result};
pub use id::{AssetId, JobId, ProjectId};
pub use time::{FrameRate, Time, TimecodeError};
