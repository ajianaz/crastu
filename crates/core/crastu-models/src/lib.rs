//! Model catalogue, resumable downloads, sha256 pins and licence metadata.
//!
//! Every entry states its licence and whether Crastu may ship it commercially. A catalogue with a
//! `Denied` entry does not load, so a non-commercial model can never slip into the shipped list.
//! User-installed engines are not catalogue entries; they live in the voice engine settings.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
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
    /// Lower-case hex sha256 of the file.
    pub sha256: String,
    pub size_bytes: Option<u64>,
    pub attribution: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("catalogue is not valid JSON: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("model {id:?}: {problem}")]
    Invalid { id: String, problem: String },
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("model {id:?}: checksum mismatch (expected {expected}, got {actual})")]
    Checksum {
        id: String,
        expected: String,
        actual: String,
    },
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

/// Opens a byte stream for a URL starting at `offset`. The HTTP adapter lives outside this crate.
pub trait Fetch {
    fn fetch(&self, url: &str, offset: u64) -> io::Result<Box<dyn Read>>;
}

/// Downloads `entry` into `dir`, resuming a previous partial download, and verifies the checksum.
/// Returns the final path. A file that is already complete and correct is not downloaded again.
pub fn download(entry: &ModelEntry, dir: &Path, fetcher: &dyn Fetch) -> Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let target = dir.join(&entry.id);
    if target.exists() && sha256_file(&target)? == entry.sha256 {
        return Ok(target);
    }
    let part = dir.join(format!("{}.part", entry.id));
    let offset = fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    let mut out = OpenOptions::new().create(true).append(true).open(&part)?;
    let mut stream = fetcher.fetch(&entry.url, offset)?;
    io::copy(&mut stream, &mut out)?;
    out.flush()?;
    drop(out);
    let actual = sha256_file(&part)?;
    if actual != entry.sha256 {
        fs::remove_file(&part)?;
        return Err(ModelError::Checksum {
            id: entry.id.clone(),
            expected: entry.sha256.clone(),
            actual,
        });
    }
    fs::rename(&part, &target)?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    const BODY: &[u8] = b"pretend these are model weights";

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
            sha256: sha(BODY),
            size_bytes: Some(BODY.len() as u64),
            attribution: None,
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
    fn parses_json() {
        let json = serde_json::to_string(&vec![entry(Commercial::Conditional {
            note: "OpenRAIL-M use restrictions".into(),
        })])
        .unwrap();
        let c = Catalogue::from_json(&json).unwrap();
        assert_eq!(c.get("whisper-small").unwrap().task, Task::SpeechToText);
    }

    /// Serves BODY from the requested offset and records the offsets asked for.
    struct Fake {
        calls: RefCell<Vec<u64>>,
        body: Vec<u8>,
    }
    impl Fetch for Fake {
        fn fetch(&self, _url: &str, offset: u64) -> io::Result<Box<dyn Read>> {
            self.calls.borrow_mut().push(offset);
            Ok(Box::new(io::Cursor::new(
                self.body[offset as usize..].to_vec(),
            )))
        }
    }

    #[test]
    fn resumes_partial_downloads_and_verifies() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("whisper-small.part"), &BODY[..10]).unwrap();
        let fake = Fake {
            calls: RefCell::new(vec![]),
            body: BODY.to_vec(),
        };
        let path = download(&entry(Commercial::Allowed), dir.path(), &fake).unwrap();
        assert_eq!(fs::read(&path).unwrap(), BODY);
        assert_eq!(*fake.calls.borrow(), vec![10]);
        download(&entry(Commercial::Allowed), dir.path(), &fake).unwrap();
        assert_eq!(
            fake.calls.borrow().len(),
            1,
            "complete file is not fetched again"
        );
    }

    #[test]
    fn checksum_mismatch_removes_the_partial_file() {
        let dir = tempfile::tempdir().unwrap();
        let fake = Fake {
            calls: RefCell::new(vec![]),
            body: b"tampered".to_vec(),
        };
        let err = download(&entry(Commercial::Allowed), dir.path(), &fake).unwrap_err();
        assert!(matches!(err, ModelError::Checksum { .. }));
        assert!(!dir.path().join("whisper-small.part").exists());
    }
}
