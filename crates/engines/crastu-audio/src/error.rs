use std::fmt;

/// What can go wrong when opening or reading an audio source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AudioError {
    /// No input device, or none with the requested name.
    NoDevice(Option<String>),
    /// The microphone permission is refused (the status word `Denied` of `crastu-os-perms`).
    PermissionDenied,
    /// The microphone permission is not known yet (`Unknown`): the system may prompt on start.
    PermissionUnknown,
    /// The device offers no sample format this crate reads.
    UnsupportedFormat(String),
    /// The device or the stream failed.
    Device(String),
    /// A WAV file that cannot be read: what is wrong with it.
    Wav(String),
    /// The source was used out of order (read before start, started twice).
    State(&'static str),
}

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AudioError::NoDevice(None) => write!(f, "no audio input device"),
            AudioError::NoDevice(Some(n)) => write!(f, "no audio input device named {n:?}"),
            AudioError::PermissionDenied => write!(f, "microphone access is denied"),
            AudioError::PermissionUnknown => write!(f, "microphone access has not been decided"),
            AudioError::UnsupportedFormat(s) => write!(f, "unsupported audio format: {s}"),
            AudioError::Device(s) => write!(f, "audio device error: {s}"),
            AudioError::Wav(s) => write!(f, "invalid WAV file: {s}"),
            AudioError::State(s) => write!(f, "audio source misuse: {s}"),
        }
    }
}

impl std::error::Error for AudioError {}
