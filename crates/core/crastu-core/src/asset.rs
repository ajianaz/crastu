use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{AssetId, Time};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    Video,
    Audio,
    Image,
    Subtitle,
}

/// A file Crastu knows about. The path is where the media lives on this machine.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AssetRef {
    pub id: AssetId,
    pub kind: AssetKind,
    pub path: PathBuf,
    /// Known after probing; `None` for images or before probing.
    pub duration: Option<Time>,
}

impl AssetKind {
    /// Guesses the kind from a file extension (case-insensitive).
    pub fn from_extension(ext: &str) -> Option<AssetKind> {
        let ext = ext.to_ascii_lowercase();
        Some(match ext.as_str() {
            "mp4" | "mov" | "mkv" | "webm" | "m4v" => AssetKind::Video,
            "wav" | "mp3" | "m4a" | "aac" | "flac" | "ogg" | "opus" => AssetKind::Audio,
            "png" | "jpg" | "jpeg" | "webp" | "gif" => AssetKind::Image,
            "srt" | "vtt" | "ass" => AssetKind::Subtitle,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_from_extension() {
        assert_eq!(AssetKind::from_extension("MOV"), Some(AssetKind::Video));
        assert_eq!(AssetKind::from_extension("wav"), Some(AssetKind::Audio));
        assert_eq!(AssetKind::from_extension("vtt"), Some(AssetKind::Subtitle));
        assert_eq!(AssetKind::from_extension("exe"), None);
    }
}
