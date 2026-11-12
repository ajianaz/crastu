//! File plumbing for a session: private files, the session folder with its lock, and line
//! streams that survive a failed write.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// The lock file each live recorder holds for its lifetime; see [`create_session`].
pub const LOCK_FILE: &str = "session.lock";

fn private_options() -> OpenOptions {
    let mut o = OpenOptions::new();
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600);
    o
}

/// Opens `path` for appending, creating it with mode 0600.
pub fn open_append(path: &Path) -> io::Result<File> {
    private_options().create(true).append(true).open(path)
}

/// Replaces the content of `path` (mode 0600 when it is created).
pub fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    private_options()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)?
        .write_all(bytes)
}

fn dir_builder(recursive: bool) -> fs::DirBuilder {
    let mut b = fs::DirBuilder::new();
    b.recursive(recursive);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut b, 0o700);
    b
}

/// A session folder and the lock that marks it as alive.
#[derive(Debug)]
pub struct Session {
    pub dir: PathBuf,
    /// Held for the life of the recorder. `None` only where the file system has no locks.
    pub lock: Option<File>,
}

/// Creates `<app_dir>/<stamp>-<pid>` (with a `-2`, `-3`... suffix if the name is taken) and takes
/// an exclusive lock on `session.lock` inside it.
///
/// The lock is how a later run knows this folder belongs to a running process: `prune` skips a
/// folder whose lock cannot be taken. It is released by the operating system when the process
/// ends, however it ends. If a folder is pruned between its creation and the lock (the lock file
/// cannot be created) or a pruner holds the lock at that moment, the empty folder is removed and
/// the next name is tried. Where the file system has no locks at all the session is recorded
/// without one, and `prune` falls back to the rule in [`is_live`].
pub fn create_session(app_dir: &Path, stamp: &str) -> Result<Session, String> {
    dir_builder(true)
        .create(app_dir)
        .map_err(|e| format!("cannot create {}: {e}", app_dir.display()))?;
    let base = format!("{stamp}-{}", std::process::id());
    for n in 1..100 {
        let name = if n == 1 {
            base.clone()
        } else {
            format!("{base}-{n}")
        };
        let dir = app_dir.join(name);
        match dir_builder(false).create(&dir) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("cannot create {}: {e}", dir.display())),
        }
        let file = match private_options()
            .create(true)
            .append(true)
            .open(dir.join(LOCK_FILE))
        {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let _ = fs::remove_dir_all(&dir);
                continue;
            }
            Err(e) => return Err(format!("cannot create {}: {e}", dir.display())),
        };
        match file.try_lock() {
            Ok(()) => {
                return Ok(Session {
                    dir,
                    lock: Some(file),
                });
            }
            Err(TryLockError::WouldBlock) => {
                let _ = fs::remove_dir_all(&dir);
                continue;
            }
            // No lock support here: record anyway (see `is_live` for how it is pruned).
            Err(TryLockError::Error(_)) => return Ok(Session { dir, lock: None }),
        }
    }
    Err(format!(
        "no free session folder name in {}",
        app_dir.display()
    ))
}

/// A session younger than this is never treated as dead when locks are unavailable.
const FALLBACK_MIN_AGE: Duration = Duration::from_secs(3600);
/// Without locks, a session that never wrote `end.json` is presumed dead only after this long.
const FALLBACK_DEAD_AGE: Duration = Duration::from_secs(7 * 24 * 3600);

/// Whether the session in `dir` belongs to a running process.
///
/// Normally its lock cannot be taken (`WouldBlock`). A folder without a lock file is not alive.
/// Where the file system has no locks (some network or FUSE homes) the lock tells nothing, and a
/// conservative rule applies, see [`fallback_live`]. Any other trouble opening the lock file
/// counts as alive, so that a doubt never deletes a running session.
pub fn is_live(dir: &Path) -> bool {
    let lock_path = dir.join(LOCK_FILE);
    match File::open(&lock_path) {
        Ok(f) => live_after_lock(f.try_lock(), dir, &lock_path, SystemTime::now()),
        Err(e) => e.kind() != io::ErrorKind::NotFound,
    }
}

/// What an attempt to take the lock says about the session: taken means nobody holds it (dead),
/// `WouldBlock` means a process holds it (alive), any other error means no locks (fallback).
fn live_after_lock(
    result: Result<(), TryLockError>,
    dir: &Path,
    lock_path: &Path,
    now: SystemTime,
) -> bool {
    match result {
        Ok(()) => false,
        Err(TryLockError::WouldBlock) => true,
        Err(TryLockError::Error(_)) => fallback_live(dir, lock_path, now),
    }
}

/// The rule where locks do not work: alive if the folder is younger than an hour; after that,
/// dead if it has an `end.json`, and otherwise alive for a week (a long-running host never gets
/// pruned out from under itself within that time, and a crashed session is then kept as old
/// evidence under the normal limits). The consequence on such file systems: a session that was
/// killed instead of ending stays for a week before the limits can remove it.
fn fallback_live(dir: &Path, lock_path: &Path, now: SystemTime) -> bool {
    let age = fs::metadata(lock_path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| now.duration_since(t).ok());
    match age {
        None => true,
        Some(age) if age < FALLBACK_MIN_AGE => true,
        Some(_) if dir.join("end.json").exists() => false,
        Some(age) => age < FALLBACK_DEAD_AGE,
    }
}

/// The last line of a stream that reached its size cap.
const CAP_MARKER: &str = r#"{"kind":"marker","msg":"size limit reached; later lines are dropped"}"#;

/// An append-only stream of lines. A failed write is counted and does not end the stream: the
/// file is reopened on the next write, and a line cut short by the failure is closed off first.
/// A stream has a size cap: when a line would pass it, one marker line is written and every later
/// line is dropped and counted, so a live session cannot fill the disk.
#[derive(Debug)]
pub struct Stream {
    path: Option<PathBuf>,
    file: Option<File>,
    dirty: bool,
    cap: u64,
    written: u64,
    capped: bool,
    pub dropped: u64,
}

impl Stream {
    pub fn open(path: Option<PathBuf>, cap: u64) -> Stream {
        let file = path.as_deref().and_then(|p| open_append(p).ok());
        Stream {
            path,
            file,
            dirty: false,
            cap,
            written: 0,
            capped: false,
            dropped: 0,
        }
    }

    #[cfg(test)]
    pub fn with_file(path: PathBuf, file: File) -> Stream {
        let mut s = Stream::open(None, u64::MAX);
        s.path = Some(path);
        s.file = Some(file);
        s
    }

    /// Writes `line` and a newline with one `write` call, straight to the file (no buffer).
    pub fn write_line(&mut self, line: &str) {
        if self.capped {
            self.dropped += 1;
            return;
        }
        if self.written.saturating_add(line.len() as u64 + 1) > self.cap {
            self.capped = true;
            self.dropped += 1;
            self.append(CAP_MARKER);
            return;
        }
        self.append(line);
    }

    fn append(&mut self, line: &str) {
        if self.file.is_none() {
            self.file = self.path.as_deref().and_then(|p| open_append(p).ok());
        }
        let Some(file) = self.file.as_mut() else {
            self.dropped += 1;
            return;
        };
        let mut buf = String::with_capacity(line.len() + 2);
        if self.dirty {
            buf.push('\n');
        }
        buf.push_str(line);
        buf.push('\n');
        if file.write_all(buf.as_bytes()).is_err() {
            self.file = None;
            self.dirty = true;
            self.dropped += 1;
        } else {
            self.dirty = false;
            self.written = self.written.saturating_add(buf.len() as u64);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stream_without_a_file_retries_and_counts_what_it_dropped() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("later/events.jsonl");
        let mut s = Stream::open(Some(path.clone()), u64::MAX);
        s.write_line("lost");
        s.write_line("lost too");
        assert_eq!(s.dropped, 2);
        fs::create_dir(d.path().join("later")).unwrap();
        s.write_line("kept");
        assert_eq!(s.dropped, 2);
        assert_eq!(fs::read_to_string(path).unwrap(), "kept\n");
    }

    #[test]
    fn a_failed_write_does_not_end_the_stream() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("events.jsonl");
        fs::write(&path, "first\n").unwrap();
        // A file opened read-only: the write fails.
        let mut s = Stream::with_file(path.clone(), File::open(&path).unwrap());
        s.write_line("fails");
        assert_eq!(s.dropped, 1);
        s.write_line("works");
        assert_eq!(s.dropped, 1);
        // The failure may have left half a line, so the next line starts on a fresh one.
        assert_eq!(fs::read_to_string(path).unwrap(), "first\n\nworks\n");
    }

    #[test]
    fn a_stream_stops_at_its_cap_with_one_marker_and_counts_the_rest() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("events.jsonl");
        let mut s = Stream::open(Some(path.clone()), 20);
        s.write_line("0123456789"); // 11 bytes with the newline
        s.write_line("0123456789"); // would make 22: over the cap
        s.write_line("later");
        s.write_line("later too");
        assert_eq!(s.dropped, 3);
        let text = fs::read_to_string(path).unwrap();
        let lines: Vec<_> = text.lines().collect();
        assert_eq!(lines.len(), 2, "{text}");
        assert_eq!(lines[0], "0123456789");
        assert!(lines[1].contains("size limit reached"), "{text}");
    }

    #[test]
    fn without_locks_a_young_session_is_alive_and_an_old_one_follows_end_json() {
        let d = tempfile::tempdir().unwrap();
        let s = create_session(d.path(), "20261112T000000Z").unwrap();
        let lock = s.dir.join(LOCK_FILE);
        let now = SystemTime::now();
        let later = |by: Duration| now + by;
        // Younger than an hour: alive whatever else is true.
        fs::write(s.dir.join("end.json"), "{}").unwrap();
        assert!(fallback_live(&s.dir, &lock, later(Duration::from_secs(60))));
        // Older, with end.json: dead.
        assert!(!fallback_live(
            &s.dir,
            &lock,
            later(Duration::from_secs(7200))
        ));
        // Older, no end.json: alive for a week, then dead.
        fs::remove_file(s.dir.join("end.json")).unwrap();
        assert!(fallback_live(
            &s.dir,
            &lock,
            later(Duration::from_secs(7200))
        ));
        assert!(fallback_live(
            &s.dir,
            &lock,
            later(Duration::from_secs(6 * 24 * 3600))
        ));
        assert!(!fallback_live(
            &s.dir,
            &lock,
            later(Duration::from_secs(8 * 24 * 3600))
        ));
        // A lock attempt that fails for another reason than "held" takes this path too.
        let unsupported = || TryLockError::Error(io::Error::from(io::ErrorKind::Unsupported));
        let at = later(Duration::from_secs(7200));
        fs::write(s.dir.join("end.json"), "{}").unwrap();
        assert!(!live_after_lock(Err(unsupported()), &s.dir, &lock, at));
        assert!(live_after_lock(
            Err(TryLockError::WouldBlock),
            &s.dir,
            &lock,
            at
        ));
        assert!(!live_after_lock(Ok(()), &s.dir, &lock, at));
        // No metadata: alive.
        assert!(fallback_live(&s.dir, &d.path().join("nope"), now));
    }

    #[test]
    fn the_session_lock_marks_a_folder_live_until_it_is_dropped() {
        let d = tempfile::tempdir().unwrap();
        let s = create_session(d.path(), "20261112T000000Z").unwrap();
        assert!(is_live(&s.dir));
        let dir = s.dir.clone();
        drop(s);
        assert!(!is_live(&dir));
        // No lock file: not alive.
        let bare = d.path().join("bare");
        fs::create_dir(&bare).unwrap();
        assert!(!is_live(&bare));
    }

    #[test]
    fn a_name_that_is_taken_gets_a_suffix() {
        let d = tempfile::tempdir().unwrap();
        let a = create_session(d.path(), "20261112T000000Z").unwrap();
        let b = create_session(d.path(), "20261112T000000Z").unwrap();
        assert_ne!(a.dir, b.dir);
    }

    #[cfg(unix)]
    #[test]
    fn private_files_are_mode_0600() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("f");
        write_private(&p, b"x").unwrap();
        assert_eq!(
            fs::metadata(&p).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let a = d.path().join("g");
        drop(open_append(&a).unwrap());
        assert_eq!(
            fs::metadata(&a).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
