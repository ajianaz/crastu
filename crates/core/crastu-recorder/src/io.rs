//! File plumbing for a session: private files, the session folder with its lock, and line
//! streams that survive a failed write.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// The lock file each live recorder holds for its lifetime; see [`create_session`].
pub(crate) const LOCK_FILE: &str = "session.lock";

fn private_options() -> OpenOptions {
    let mut o = OpenOptions::new();
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600);
    o
}

/// Opens `path` for appending, creating it with mode 0600.
pub(crate) fn open_append(path: &Path) -> io::Result<File> {
    private_options().create(true).append(true).open(path)
}

/// Replaces the content of `path` (mode 0600 when it is created).
pub(crate) fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
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
pub(crate) struct Session {
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
/// cannot be created) or a pruner holds the lock at that moment, the next name is tried.
pub(crate) fn create_session(app_dir: &Path, stamp: &str) -> Result<Session, String> {
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
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(format!("cannot create {}: {e}", dir.display())),
        };
        match file.try_lock() {
            Ok(()) => {
                return Ok(Session {
                    dir,
                    lock: Some(file),
                });
            }
            Err(TryLockError::WouldBlock) => continue,
            // No lock support here: record anyway; pruning cannot tell this session is alive.
            Err(TryLockError::Error(_)) => return Ok(Session { dir, lock: None }),
        }
    }
    Err(format!(
        "no free session folder name in {}",
        app_dir.display()
    ))
}

/// Whether the session in `dir` belongs to a running process: its lock cannot be taken. A folder
/// without a lock file is not alive. Any other trouble counts as alive, so that a doubt never
/// deletes a running session.
pub(crate) fn is_live(dir: &Path) -> bool {
    match File::open(dir.join(LOCK_FILE)) {
        Ok(f) => !matches!(f.try_lock(), Ok(())),
        Err(e) => e.kind() != io::ErrorKind::NotFound,
    }
}

/// An append-only stream of lines. A failed write is counted and does not end the stream: the
/// file is reopened on the next write, and a line cut short by the failure is closed off first.
#[derive(Debug)]
pub(crate) struct Stream {
    path: Option<PathBuf>,
    file: Option<File>,
    dirty: bool,
    pub dropped: u64,
}

impl Stream {
    pub(crate) fn open(path: Option<PathBuf>) -> Stream {
        let file = path.as_deref().and_then(|p| open_append(p).ok());
        Stream {
            path,
            file,
            dirty: false,
            dropped: 0,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_file(path: PathBuf, file: File) -> Stream {
        Stream {
            path: Some(path),
            file: Some(file),
            dirty: false,
            dropped: 0,
        }
    }

    /// Writes `line` and a newline with one `write` call, straight to the file (no buffer).
    pub(crate) fn write_line(&mut self, line: &str) {
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
        let mut s = Stream::open(Some(path.clone()));
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
