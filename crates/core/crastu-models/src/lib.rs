//! Model catalogue, resumable downloads, sha256 pins and licence metadata.
//!
//! Every entry states its licence and whether Crastu may ship it commercially. A catalogue with a
//! `Denied` entry does not load, so a non-commercial model can never slip into the shipped list.
//! User-installed engines are not catalogue entries; they live in the voice engine settings.
//!
//! [`catalogue`] loads the entries that ship with Crastu (`catalogue.json`); [`download`] fetches
//! one through a [`Fetch`] adapter, resuming, verifying the sha256 and renaming atomically.

mod download;
mod names;

pub use download::{Cancel, Fetch, FetchResponse, Progress, download, download_with};

use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Commercial {
    /// Plain commercial use (MIT, Apache-2.0, CC-BY-4.0 with attribution, ...).
    Allowed,
    /// Commercial use with extra terms shown to the user (OpenRAIL-M, Gemma Terms of Use, ...).
    Conditional { note: String },
    /// Not shippable (non-commercial, gated research licences, ...).
    Denied,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Task {
    SpeechToText,
    VoiceActivity,
    TextToSpeech,
    FaceDetection,
    Embedding,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelEntry {
    pub id: String,
    pub name: String,
    pub task: Task,
    /// SPDX id where one exists, otherwise the licence's own name.
    pub licence: String,
    pub licence_url: String,
    pub commercial: Commercial,
    pub url: String,
    /// The name the file gets in the download folder (no path; may be an archive name).
    pub file_name: String,
    /// Lower-case hex sha256 of the file.
    pub sha256: String,
    pub size_bytes: Option<u64>,
    pub attribution: Option<String>,
    /// A line for the user or the next slice (for example that the download is an archive).
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("catalogue is not valid JSON: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("model {id:?}: {problem}")]
    Invalid { id: String, problem: String },
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("model {id:?}: expected {expected} bytes, got {actual}")]
    Size {
        id: String,
        expected: u64,
        actual: u64,
    },
    #[error("model {id:?}: already downloading, or a stale lock: remove {}", path.display())]
    Locked { id: String, path: PathBuf },
    #[error("the download was cancelled")]
    Cancelled,
    #[error("model {id:?}: the server answered with an unusable range: {problem}")]
    Protocol { id: String, problem: String },
    #[error("model {id:?}: checksum mismatch (expected {expected}, got {actual})")]
    Checksum {
        id: String,
        expected: String,
        actual: String,
    },
}

const BUILTIN: &str = include_str!("catalogue.json");

/// The catalogue that ships with Crastu. Panics only if the embedded JSON is invalid, which a
/// test of this crate rules out.
pub fn catalogue() -> Catalogue {
    Catalogue::from_json(BUILTIN).expect("the embedded model catalogue is valid")
}

pub type Result<T> = std::result::Result<T, ModelError>;

#[derive(Clone, Debug, Default)]
pub struct Catalogue {
    entries: Vec<ModelEntry>,
}

impl Catalogue {
    pub fn from_json(json: &str) -> Result<Catalogue> {
        let entries: Vec<ModelEntry> = serde_json::from_str(json)?;
        Self::new(entries)
    }

    pub fn new(entries: Vec<ModelEntry>) -> Result<Catalogue> {
        let mut seen = std::collections::HashSet::new();
        for e in &entries {
            let bad = |problem: &str| ModelError::Invalid {
                id: e.id.clone(),
                problem: problem.to_string(),
            };
            if !seen.insert(e.id.as_str()) {
                return Err(bad("duplicate id"));
            }
            if e.commercial == Commercial::Denied {
                return Err(bad(
                    "licence does not allow commercial use; it cannot be in the shipped catalogue",
                ));
            }
            if let Commercial::Conditional { note } = &e.commercial
                && note.trim().is_empty()
            {
                return Err(bad("conditional licences need a note for the user"));
            }
            if e.licence.trim().is_empty() || !e.licence_url.starts_with("https://") {
                return Err(bad("licence and an https licence_url are required"));
            }
            if !e.url.starts_with("https://") {
                return Err(bad("download url must be https"));
            }
            if let Some(problem) = names::problem(&e.file_name) {
                return Err(bad(problem));
            }
            if e.sha256.len() != 64
                || !e
                    .sha256
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
            {
                return Err(bad("sha256 must be 64 lower-case hex characters"));
            }
        }
        Ok(Catalogue { entries })
    }

    pub fn entries(&self) -> &[ModelEntry] {
        &self.entries
    }

    pub fn get(&self, id: &str) -> Option<&ModelEntry> {
        self.entries.iter().find(|e| e.id == id)
    }
}

/// Streams a file through sha256 and returns lower-case hex.
pub fn sha256_file(path: &Path) -> io::Result<String> {
    let mut f = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sha(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    fn entry(commercial: Commercial) -> ModelEntry {
        ModelEntry {
            id: "whisper-small".into(),
            name: "Whisper Small".into(),
            task: Task::SpeechToText,
            licence: "MIT".into(),
            licence_url: "https://example.org/licence".into(),
            commercial,
            url: "https://example.org/model.bin".into(),
            file_name: "model.bin".into(),
            sha256: sha(b"weights"),
            size_bytes: Some(7),
            attribution: None,
            note: None,
        }
    }

    #[test]
    fn denied_and_malformed_entries_are_rejected() {
        assert!(Catalogue::new(vec![entry(Commercial::Allowed)]).is_ok());
        assert!(Catalogue::new(vec![entry(Commercial::Denied)]).is_err());
        assert!(Catalogue::new(vec![entry(Commercial::Conditional { note: " ".into() })]).is_err());
        let mut e = entry(Commercial::Allowed);
        e.sha256 = "ABC".into();
        assert!(Catalogue::new(vec![e]).is_err());
        assert!(
            Catalogue::new(vec![entry(Commercial::Allowed), entry(Commercial::Allowed)]).is_err()
        );
    }

    #[test]
    fn file_names_cannot_escape_the_download_folder() {
        for bad in [
            "", "..", "../x", "a/b", "C:foo", "NUL", "name.", "m.PART", "m.Lock",
        ] {
            let mut e = entry(Commercial::Allowed);
            e.file_name = bad.into();
            assert!(Catalogue::new(vec![e]).is_err(), "{bad:?} must be rejected");
        }
    }

    #[test]
    fn parses_json() {
        let json = serde_json::to_string(&vec![entry(Commercial::Conditional {
            note: "OpenRAIL-M use restrictions".into(),
        })])
        .unwrap();
        let c = Catalogue::from_json(&json).unwrap();
        assert_eq!(c.get("whisper-small").unwrap().task, Task::SpeechToText);
    }

    #[test]
    fn a_denied_entry_in_json_does_not_load() {
        let mut denied = serde_json::to_value(entry(Commercial::Allowed)).unwrap();
        denied["commercial"] = serde_json::json!({"status": "denied"});
        let json = serde_json::to_string(&vec![denied]).unwrap();
        assert!(matches!(
            Catalogue::from_json(&json),
            Err(ModelError::Invalid { .. })
        ));
    }

    #[test]
    fn the_shipped_catalogue_loads_and_every_entry_allows_commercial_use() {
        let c = catalogue();
        for id in ["whisper-small", "silero-vad", "yunet"] {
            let e = c
                .get(id)
                .unwrap_or_else(|| panic!("{id} is in the catalogue"));
            assert_ne!(e.commercial, Commercial::Denied);
            assert!(e.size_bytes.is_some_and(|n| n > 0), "{id} has a size");
        }
        assert!(
            c.get("supertonic-3").is_none(),
            "Supertonic stays out (open question 3)"
        );
    }
}
