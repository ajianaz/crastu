//! Offline mode for the downloader: [`GatedFetch`] asks the [`NetGate`] before any request is
//! opened, and a model that is already on disk keeps working without one.

use std::io;
use std::path::{Path, PathBuf};

use crastu_core::{NetGate, Offline};

use crate::{Fetch, FetchResponse, ModelEntry, ModelError, Result, sha256_file};

/// Wraps a [`Fetch`]: with Offline on, the request is refused before the inner adapter is
/// called, so no socket is opened. The refusal comes back from `download_with` as
/// [`ModelError::Offline`].
#[derive(Debug)]
pub struct GatedFetch<F> {
    inner: F,
    gate: NetGate,
    what: String,
}

impl<F: Fetch> GatedFetch<F> {
    /// `model_id` names the download in the refusal (`model download <id>`).
    pub fn new(inner: F, gate: NetGate, model_id: &str) -> GatedFetch<F> {
        GatedFetch {
            inner,
            gate,
            what: format!("model download {model_id}"),
        }
    }
}

impl<F: Fetch> Fetch for GatedFetch<F> {
    fn fetch(&self, url: &str, offset: u64) -> io::Result<FetchResponse> {
        self.gate
            .check(&self.what, url)
            .map_err(|refusal| io::Error::new(io::ErrorKind::PermissionDenied, refusal))?;
        self.inner.fetch(url, offset)
    }
}

/// The refusal travels through `io::Error` (the `Fetch` signature); this turns it back.
pub(crate) fn offline_from(e: &io::Error) -> Option<Offline> {
    e.get_ref()?.downcast_ref::<Offline>().cloned()
}

fn target(entry: &ModelEntry, dir: &Path) -> PathBuf {
    dir.join(&entry.file_name)
}

/// A quick check for listings: the file is there and has the pinned size. It does not hash the
/// file; [`require_installed`] does.
pub fn is_present(entry: &ModelEntry, dir: &Path) -> bool {
    match std::fs::metadata(target(entry, dir)) {
        Ok(m) => m.is_file() && entry.size_bytes.is_none_or(|s| s == m.len()),
        Err(_) => false,
    }
}

/// The path of an installed model, checked against its sha256, or
/// [`ModelError::NotInstalled`] saying how to get it. Makes no request.
pub fn require_installed(entry: &ModelEntry, dir: &Path) -> Result<PathBuf> {
    let path = target(entry, dir);
    let missing = || ModelError::NotInstalled {
        id: entry.id.clone(),
        name: entry.name.clone(),
    };
    if !is_present(entry, dir) {
        return Err(missing());
    }
    if sha256_file(&path)? != entry.sha256 {
        return Err(missing());
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::io::Cursor;

    use sha2::{Digest, Sha256};

    use super::*;
    use crate::{Commercial, Task, download};

    /// Counts requests; `panic_on_open` stands for "a socket would have been opened".
    struct Server {
        body: Vec<u8>,
        opened: Cell<u32>,
        panic_on_open: bool,
    }

    impl Fetch for Server {
        fn fetch(&self, _: &str, offset: u64) -> io::Result<FetchResponse> {
            assert!(!self.panic_on_open, "a request was opened while offline");
            self.opened.set(self.opened.get() + 1);
            Ok(FetchResponse {
                reader: Box::new(Cursor::new(self.body[offset as usize..].to_vec())),
                start: offset,
                total: Some(self.body.len() as u64),
            })
        }
    }

    fn server(body: &[u8], panic_on_open: bool) -> Server {
        Server {
            body: body.to_vec(),
            opened: Cell::new(0),
            panic_on_open,
        }
    }

    fn entry(body: &[u8]) -> ModelEntry {
        let sha: String = Sha256::digest(body)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        ModelEntry {
            id: "whisper-small".into(),
            name: "Whisper Small".into(),
            task: Task::SpeechToText,
            licence: "MIT".into(),
            licence_url: "https://example.org/l".into(),
            commercial: Commercial::Allowed,
            url: "https://user:pw@huggingface.co/m.bin?token=abc".into(),
            file_name: "m.bin".into(),
            sha256: sha,
            size_bytes: Some(body.len() as u64),
            attribution: None,
            note: None,
        }
    }

    #[test]
    fn offline_refuses_before_any_request_and_leaves_no_download_behind() {
        let body = vec![5u8; 5000];
        let dir = tempfile::tempdir().unwrap();
        let gate = NetGate::new(true);
        let e = entry(&body);
        let fetch = GatedFetch::new(server(&body, true), gate.clone(), &e.id);
        let err = download(&e, dir.path(), &fetch).unwrap_err();
        assert!(matches!(err, ModelError::Offline(_)), "{err:?}");
        assert_eq!(
            err.to_string(),
            "Offline: model download whisper-small from huggingface.co refused"
        );
        assert!(!dir.path().join("m.bin").exists());
        assert!(!dir.path().join("m.bin.part").exists());
        let kept = gate.refusals();
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].host, "huggingface.co");
    }

    #[test]
    fn online_downloads_through_the_gate() {
        let body = vec![5u8; 5000];
        let dir = tempfile::tempdir().unwrap();
        let e = entry(&body);
        let inner = server(&body, false);
        let fetch = GatedFetch::new(inner, NetGate::new(false), &e.id);
        let path = download(&e, dir.path(), &fetch).unwrap();
        assert_eq!(std::fs::read(path).unwrap(), body);
        assert_eq!(fetch.inner.opened.get(), 1);
    }

    #[test]
    fn switching_offline_on_stops_the_next_request() {
        let body = vec![5u8; 5000];
        let dir = tempfile::tempdir().unwrap();
        let e = entry(&body);
        let gate = NetGate::new(false);
        let fetch = GatedFetch::new(server(&body, false), gate.clone(), &e.id);
        gate.set_offline(true);
        assert!(download(&e, dir.path(), &fetch).is_err());
        assert_eq!(fetch.inner.opened.get(), 0);
    }

    #[test]
    fn an_installed_model_keeps_working_offline_without_a_request() {
        let body = vec![9u8; 5000];
        let dir = tempfile::tempdir().unwrap();
        let e = entry(&body);
        std::fs::write(dir.path().join("m.bin"), &body).unwrap();
        let gate = NetGate::new(true);
        let fetch = GatedFetch::new(server(&body, true), gate.clone(), &e.id);
        let path = download(&e, dir.path(), &fetch).unwrap();
        assert_eq!(path, dir.path().join("m.bin"));
        assert_eq!(require_installed(&e, dir.path()).unwrap(), path);
        assert!(is_present(&e, dir.path()));
        assert!(gate.refusals().is_empty());
    }

    #[test]
    fn a_missing_model_says_how_to_get_it() {
        let body = vec![9u8; 5000];
        let dir = tempfile::tempdir().unwrap();
        let e = entry(&body);
        let err = require_installed(&e, dir.path()).unwrap_err();
        assert_eq!(
            err.to_string(),
            "Whisper Small is not installed; turn Offline off once and run \
             `crastu models download whisper-small`"
        );
        assert!(!is_present(&e, dir.path()));
    }

    #[test]
    fn a_corrupt_or_short_file_is_not_installed() {
        let body = vec![9u8; 5000];
        let dir = tempfile::tempdir().unwrap();
        let e = entry(&body);
        std::fs::write(dir.path().join("m.bin"), vec![1u8; 5000]).unwrap();
        assert!(
            is_present(&e, dir.path()),
            "same size passes the quick check"
        );
        assert!(matches!(
            require_installed(&e, dir.path()),
            Err(ModelError::NotInstalled { .. })
        ));
        std::fs::write(dir.path().join("m.bin"), &body[..10]).unwrap();
        assert!(!is_present(&e, dir.path()));
    }

    #[test]
    fn other_io_errors_stay_io_errors() {
        let e: ModelError = io::Error::new(io::ErrorKind::PermissionDenied, "nope").into();
        assert!(matches!(e, ModelError::Io(_)));
    }
}
