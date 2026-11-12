//! Keeping the last N sessions or M megabytes, whichever is smaller.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// How much history to keep for one app. Both limits apply: a session survives only inside the
/// newest `keep` sessions and inside the newest `max_bytes` of data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Retention {
    pub keep: usize,
    pub max_bytes: u64,
}

impl Default for Retention {
    fn default() -> Self {
        Retention {
            keep: 20,
            max_bytes: 50 * 1024 * 1024,
        }
    }
}

/// Whether `name` looks like `<UTC stamp>-<pid>`: `20261112T103045Z-1234` (a `-<n>` suffix is
/// allowed). Retention only touches folders with this shape, never anything else in the root.
pub(crate) fn is_session_name(name: &str) -> bool {
    let b = name.as_bytes();
    b.len() > 17
        && b[..8].iter().all(u8::is_ascii_digit)
        && b[8] == b'T'
        && b[9..15].iter().all(u8::is_ascii_digit)
        && b[15] == b'Z'
        && b[16] == b'-'
        && b[17..].iter().all(|c| c.is_ascii_digit() || *c == b'-')
}

/// How long a session without `end.json` is presumed alive (or a fresh crash to look at).
pub(crate) const LIVE_WINDOW: Duration = Duration::from_secs(7 * 24 * 3600);

/// No `end.json`, and the folder or one of its files changed within [`LIVE_WINDOW`].
fn may_be_live(dir: &Path) -> bool {
    if dir.join("end.json").exists() {
        return false;
    }
    let newest = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.metadata().ok()?.modified().ok())
        .chain(fs::metadata(dir).and_then(|m| m.modified()).ok())
        .max();
    match newest {
        Some(t) => SystemTime::now()
            .duration_since(t)
            .map(|age| age < LIVE_WINDOW)
            .unwrap_or(true),
        None => true,
    }
}

fn dir_size(dir: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|e| match e.metadata() {
            Ok(m) if m.is_dir() => dir_size(&e.path()),
            Ok(m) => m.len(),
            Err(_) => 0,
        })
        .sum()
}

/// Deletes the sessions in `app_dir` that fall outside `retention`, oldest first by folder
/// name (the UTC stamp sorts in time order, the pid breaks ties), and returns the folders it
/// removed. `current` is never deleted and counts toward both limits. Errors are ignored: a
/// folder that cannot be removed stays for the next run.
pub fn prune(app_dir: &Path, current: &Path, retention: Retention) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(app_dir) else {
        return Vec::new();
    };
    let mut sessions: Vec<(String, PathBuf)> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            (is_session_name(&name) && e.path().is_dir()).then(|| (name, e.path()))
        })
        .collect();
    sessions.sort();

    let mut kept = 0usize;
    let mut bytes = 0u64;
    let mut doomed = Vec::new();
    for (_, path) in sessions.into_iter().rev() {
        // A session that has not ended and was written to recently may belong to a process that
        // is still running (a long-lived host next to many short commands), and a recent crash
        // is evidence: neither is deleted, and neither counts toward the limits.
        if path != current && may_be_live(&path) {
            continue;
        }
        let size = dir_size(&path);
        let is_current = path == current;
        let fits = kept < retention.keep && bytes.saturating_add(size) <= retention.max_bytes;
        if is_current || fits {
            kept += 1;
            bytes = bytes.saturating_add(size);
        } else {
            doomed.push(path);
        }
    }
    doomed.retain(|p| fs::remove_dir_all(p).is_ok());
    doomed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(app: &Path, name: &str, bytes: usize) -> PathBuf {
        let p = app.join(name);
        fs::create_dir_all(&p).unwrap();
        fs::write(p.join("events.jsonl"), vec![b'x'; bytes]).unwrap();
        // Ended cleanly, so retention may remove it.
        fs::write(p.join("end.json"), b"").unwrap();
        p
    }

    #[test]
    fn a_session_that_may_still_be_running_is_not_deleted() {
        let d = tempfile::tempdir().unwrap();
        let cur = session(d.path(), "20261110T000000Z-1", 1);
        // No end.json and written to recently: possibly alive, however old its name.
        let live = session(d.path(), "20261101T000000Z-2", 1);
        fs::remove_file(live.join("end.json")).unwrap();
        // No end.json but untouched for longer than the window: a crash from long ago.
        let stale = session(d.path(), "20261102T000000Z-3", 1);
        fs::remove_file(stale.join("end.json")).unwrap();
        let old = SystemTime::now() - LIVE_WINDOW - Duration::from_secs(60);
        let h = fs::File::options()
            .write(true)
            .open(stale.join("events.jsonl"))
            .unwrap();
        h.set_modified(old).unwrap();
        fs::File::open(&stale).unwrap().set_modified(old).unwrap();
        let r = Retention {
            keep: 1,
            max_bytes: u64::MAX,
        };
        prune(d.path(), &cur, r);
        assert_eq!(
            names(d.path()),
            ["20261101T000000Z-2", "20261110T000000Z-1"]
        );
    }

    fn names(app: &Path) -> Vec<String> {
        let mut v: Vec<_> = fs::read_dir(app)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().into_string().unwrap())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn session_names() {
        assert!(is_session_name("20261112T103045Z-1234"));
        assert!(is_session_name("20261112T103045Z-1234-2"));
        for bad in [
            "notes",
            "20261112T103045Z",
            "20261112T103045Z-",
            "x0261112T103045Z-1",
        ] {
            assert!(!is_session_name(bad), "{bad}");
        }
    }

    #[test]
    fn keeps_the_newest_n() {
        let d = tempfile::tempdir().unwrap();
        for n in 1..=5 {
            session(d.path(), &format!("2026111{n}T000000Z-1"), 10);
        }
        let cur = d.path().join("20261115T000000Z-1");
        let r = Retention {
            keep: 2,
            max_bytes: u64::MAX,
        };
        let removed = prune(d.path(), &cur, r);
        assert_eq!(removed.len(), 3);
        assert_eq!(
            names(d.path()),
            ["20261114T000000Z-1", "20261115T000000Z-1"]
        );
    }

    #[test]
    fn the_byte_limit_applies_when_it_is_smaller() {
        let d = tempfile::tempdir().unwrap();
        for n in 1..=4 {
            session(d.path(), &format!("2026111{n}T000000Z-1"), 100);
        }
        let cur = d.path().join("20261114T000000Z-1");
        let r = Retention {
            keep: 100,
            max_bytes: 250,
        };
        prune(d.path(), &cur, r);
        assert_eq!(
            names(d.path()),
            ["20261113T000000Z-1", "20261114T000000Z-1"]
        );
    }

    #[test]
    fn the_current_session_is_never_deleted() {
        let d = tempfile::tempdir().unwrap();
        // The current session is the oldest by name and bigger than the byte limit.
        let cur = session(d.path(), "20261101T000000Z-1", 1000);
        session(d.path(), "20261102T000000Z-1", 10);
        let r = Retention {
            keep: 1,
            max_bytes: 5,
        };
        prune(d.path(), &cur, r);
        assert_eq!(names(d.path()), ["20261101T000000Z-1"]);
    }

    #[test]
    fn other_folders_and_files_are_left_alone() {
        let d = tempfile::tempdir().unwrap();
        fs::create_dir(d.path().join("my-notes")).unwrap();
        fs::write(d.path().join("20261101T000000Z-1"), b"a file").unwrap();
        let cur = session(d.path(), "20261103T000000Z-1", 1);
        session(d.path(), "20261102T000000Z-1", 1);
        prune(
            d.path(),
            &cur,
            Retention {
                keep: 1,
                max_bytes: u64::MAX,
            },
        );
        assert_eq!(
            names(d.path()),
            ["20261101T000000Z-1", "20261103T000000Z-1", "my-notes"]
        );
    }

    #[test]
    fn a_missing_folder_is_not_an_error() {
        let d = tempfile::tempdir().unwrap();
        assert!(
            prune(
                &d.path().join("nope"),
                &d.path().join("x"),
                Retention::default()
            )
            .is_empty()
        );
    }
}
