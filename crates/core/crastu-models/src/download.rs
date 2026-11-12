//! The downloader: resumes a `.part` file, checks size and sha256, renames atomically.
//!
//! Nothing here touches the network by itself: bytes come from a [`Fetch`] adapter (the real one
//! is in `crastu-models-http`), so the logic runs against a local test server or in memory. A file
//! only ever gets its final name after its sha256 matched; whatever is in the way (a failed or
//! interrupted download) lives under `<file_name>.part`.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use sha2::{Digest, Sha256};

use crate::{ModelEntry, ModelError, Result, sha256_file};

/// What a ranged GET returned.
pub struct FetchResponse {
    /// The body, starting at byte `start` of the file.
    pub reader: Box<dyn Read>,
    /// Where the body starts: the requested offset when the server honoured the range, 0 when it
    /// ignored the range and sent the whole file.
    pub start: u64,
    /// Size of the whole file when the server said so.
    pub total: Option<u64>,
}

impl std::fmt::Debug for FetchResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FetchResponse")
            .field("start", &self.start)
            .field("total", &self.total)
            .finish_non_exhaustive()
    }
}

/// Opens a ranged GET (`Range: bytes=offset-`) for a URL. The HTTP adapter lives outside this
/// crate; tests use a local server or memory.
pub trait Fetch {
    fn fetch(&self, url: &str, offset: u64) -> io::Result<FetchResponse>;
}

impl<T: Fetch + ?Sized> Fetch for &T {
    fn fetch(&self, url: &str, offset: u64) -> io::Result<FetchResponse> {
        (**self).fetch(url, offset)
    }
}

/// Bytes on disk so far, out of the total when it is known.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Progress {
    pub done: u64,
    pub total: Option<u64>,
}

/// A flag the caller sets to stop a running download; the part file stays for a later resume.
#[derive(Debug, Default)]
pub struct Cancel(AtomicBool);

impl Cancel {
    pub fn new() -> Cancel {
        Cancel::default()
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

fn hex(hasher: Sha256) -> String {
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Downloads `entry` into `dir` without progress or cancel. See [`download_with`].
pub fn download(entry: &ModelEntry, dir: &Path, fetcher: &dyn Fetch) -> Result<PathBuf> {
    download_with(entry, dir, fetcher, &Cancel::new(), &mut |_| {})
}

/// Downloads `entry` into `dir`, resuming a previous partial download, and verifies size and
/// checksum. Returns the final path. A file that is already complete and correct is not
/// downloaded again.
///
/// Errors leave the folder tidy: a wrong checksum or too many bytes delete the part file; a
/// cancel or a connection that ended early keep it so the next call resumes. When a resumed part
/// does not hash to the pinned value (the file changed on the server), the download starts over
/// once from byte 0 before the checksum error is returned.
pub fn download_with(
    entry: &ModelEntry,
    dir: &Path,
    fetcher: &dyn Fetch,
    cancel: &Cancel,
    progress: &mut dyn FnMut(Progress),
) -> Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let target = dir.join(&entry.file_name);
    let part = dir.join(format!("{}.part", entry.file_name));
    let _lock = PartLock::take(entry, dir)?;
    // Checked under the lock: another process may have finished the file while this one waited
    // to start, and a good file must not be downloaded over.
    if target.is_file() && sha256_file(&target)? == entry.sha256 {
        return Ok(target);
    }
    let resumed = fs::metadata(&part).map(|m| m.len() > 0).unwrap_or(false);
    match attempt(entry, &part, fetcher, cancel, progress) {
        Err(ModelError::Checksum { .. }) if resumed => {
            attempt(entry, &part, fetcher, cancel, progress)?
        }
        other => other?,
    }
    fs::rename(&part, &target)?;
    Ok(target)
}

/// Held while a download writes the part file, so that two downloads of the same model cannot
/// truncate, append to or delete each other's part. It is an operating-system lock on
/// `<file_name>.part.lock` (`File::try_lock`), which the system releases when the process ends
/// for any reason, so a Ctrl-C, a kill or a power loss never blocks the next run. The empty lock
/// file itself stays in the folder: removing it would let a waiting process lock a deleted file
/// while a third one locks a new one.
struct PartLock {
    _held: File,
}

impl PartLock {
    fn take(entry: &ModelEntry, dir: &Path) -> Result<PartLock> {
        let path = dir.join(format!("{}.part.lock", entry.file_name));
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;
        match file.try_lock() {
            Ok(()) => Ok(PartLock { _held: file }),
            Err(std::fs::TryLockError::WouldBlock) => Err(ModelError::Locked {
                id: entry.id.clone(),
                path,
            }),
            Err(std::fs::TryLockError::Error(e)) => Err(e.into()),
        }
    }
}

/// One pass over the part file: on `Ok` the part file is complete and its sha256 matches.
fn attempt(
    entry: &ModelEntry,
    part: &Path,
    fetcher: &dyn Fetch,
    cancel: &Cancel,
    progress: &mut dyn FnMut(Progress),
) -> Result<()> {
    let protocol = |problem: String| ModelError::Protocol {
        id: entry.id.clone(),
        problem,
    };
    let mut have = fs::metadata(part).map(|m| m.len()).unwrap_or(0);
    if entry.size_bytes.is_some_and(|size| have > size) {
        // Longer than the real file: it cannot be a prefix of it.
        fs::remove_file(part)?;
        have = 0;
    }
    let known_total = entry.size_bytes;
    let mut hasher = Sha256::new();
    let mut written = have;
    if known_total == Some(have) && have > 0 {
        // Everything is on disk already; only the checksum is left.
        feed_from_file(part, &mut hasher)?;
    } else {
        let mut response = fetcher.fetch(&entry.url, have)?;
        if let (Some(total), Some(size)) = (response.total, entry.size_bytes)
            && total != size
        {
            return Err(ModelError::Size {
                id: entry.id.clone(),
                expected: size,
                actual: total,
            });
        }
        let total = entry.size_bytes.or(response.total);
        let mut file = if response.start == 0 {
            // From the start: either nothing was on disk or the server ignored the range.
            OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(part)?
        } else if response.start == have {
            feed_from_file(part, &mut hasher)?;
            OpenOptions::new().append(true).open(part)?
        } else {
            return Err(protocol(format!(
                "asked for byte {have}, the body starts at {}",
                response.start
            )));
        };
        written = response.start;
        let mut buf = vec![0u8; 64 * 1024];
        progress(Progress {
            done: written,
            total,
        });
        loop {
            if cancel.is_cancelled() {
                file.flush()?;
                return Err(ModelError::Cancelled);
            }
            // Read one byte more than announced so that a longer body is noticed.
            let room = match total {
                Some(t) => (t.saturating_sub(written) + 1).min(buf.len() as u64) as usize,
                None => buf.len(),
            };
            let n = response.reader.read(&mut buf[..room])?;
            if n == 0 {
                break;
            }
            written += n as u64;
            if let Some(t) = total
                && written > t
            {
                drop(file);
                fs::remove_file(part)?;
                return Err(ModelError::Size {
                    id: entry.id.clone(),
                    expected: t,
                    actual: written,
                });
            }
            file.write_all(&buf[..n])?;
            hasher.update(&buf[..n]);
            progress(Progress {
                done: written,
                total,
            });
        }
        file.flush()?;
        if let Some(t) = total
            && written < t
        {
            // The connection ended early: keep what is there, the next call resumes.
            return Err(ModelError::Size {
                id: entry.id.clone(),
                expected: t,
                actual: written,
            });
        }
        file.sync_all()?;
    }
    let actual = hex(hasher);
    if actual != entry.sha256 {
        fs::remove_file(part)?;
        return Err(ModelError::Checksum {
            id: entry.id.clone(),
            expected: entry.sha256.clone(),
            actual,
        });
    }
    progress(Progress {
        done: written,
        total: Some(written),
    });
    Ok(())
}

/// Feeds the bytes already on disk to the hasher.
fn feed_from_file(path: &Path, hasher: &mut Sha256) -> io::Result<()> {
    let mut f = File::open(path)?;
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            return Ok(());
        }
        hasher.update(&buf[..n]);
    }
}

#[cfg(test)]
#[path = "download_tests.rs"]
mod tests;
