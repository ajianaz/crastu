//! The panic hook: `panic.txt` with the reason and the last events, then the previous hook.
//!
//! Code here runs while the process is already panicking, and a panic inside a panic hook
//! aborts the process; `catch_unwind` cannot contain it, because the runtime aborts first. So
//! the only protection is to write code that cannot panic: no `unwrap`, no indexing, no
//! arithmetic that can overflow, `try_lock` instead of `lock` (the panicking thread may hold the
//! state lock), and every I/O error ignored.

use std::backtrace::Backtrace;
use std::fmt::Write as _;
use std::io::Write as _;
use std::panic::{self, PanicHookInfo};
use std::sync::atomic::Ordering;
use std::sync::{Arc, TryLockError, Weak};

use crate::time::Utc;
use crate::{Inner, PANIC_FILE, RING_DUMP};
use crastu_logfiles::open_append;
use crastu_redact::clean;

fn payload_text(info: &PanicHookInfo<'_>) -> String {
    let p = info.payload();
    if let Some(s) = p.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = p.downcast_ref::<String>() {
        s.clone()
    } else {
        "(non-text panic payload)".to_string()
    }
}

/// Panics reported in full (backtrace and last events); later ones get one line each.
const MAX_FULL_PANICS: u64 = 5;
/// No more is appended to `panic.txt` once it is this big.
const PANIC_FILE_CAP: u64 = 256 * 1024;

/// Whether `path` is still smaller than `cap` bytes (a missing file is).
fn under_cap(path: &std::path::Path, cap: u64) -> bool {
    std::fs::metadata(path).map_or(0, |m| m.len()) < cap
}

impl Inner {
    /// Appends a report for `info` to `panic.txt`, and marks a run that had already finished as
    /// panicked. A job runner that catches panics in a loop must not fill the disk, so only the
    /// first few panics are written in full, later ones as one line, and nothing at all once the
    /// file is 256 KB; every panic is still counted. Never panics; does nothing without a folder.
    fn write_panic(&self, info: &PanicHookInfo<'_>) {
        let n = self.panics.fetch_add(1, Ordering::SeqCst).saturating_add(1);
        let Some(dir) = &self.dir else { return };
        let thread = std::thread::current();
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown".to_string());
        let thread_name = thread.name().unwrap_or("<unnamed>");
        // A poisoned lock still holds the events: a panic that poisoned it is the usual reason
        // for the next panic report, and those are the events worth having.
        let state = match self.state.try_lock() {
            Ok(g) => Some(g),
            Err(TryLockError::Poisoned(p)) => Some(p.into_inner()),
            Err(TryLockError::WouldBlock) => None,
        };
        let mut text = String::new();
        if n <= MAX_FULL_PANICS {
            let _ = writeln!(text, "crastu panic #{n} at {}", Utc::now().rfc3339());
            let _ = writeln!(text, "thread: {thread_name}");
            let _ = writeln!(text, "location: {location}");
            let _ = writeln!(text, "message: {}", clean(&payload_text(info), 2000));
            let _ = writeln!(text, "\nbacktrace:\n{}", Backtrace::force_capture());
            match &state {
                Some(state) => {
                    let skip = state.ring.len().saturating_sub(RING_DUMP);
                    let shown = state.ring.len().saturating_sub(skip);
                    let _ = writeln!(text, "last events ({shown}):");
                    for line in state.ring.iter().skip(skip) {
                        let _ = writeln!(text, "{line}");
                    }
                }
                None => {
                    let _ = writeln!(text, "last events: unavailable (the event log was in use)");
                }
            }
            let _ = writeln!(text, "---");
        } else {
            let _ = writeln!(
                text,
                "panic #{n} at {} thread {thread_name} at {location}: {}",
                Utc::now().rfc3339(),
                clean(&payload_text(info), 160)
            );
        }
        let path = dir.join(PANIC_FILE);
        if under_cap(&path, PANIC_FILE_CAP)
            && let Ok(mut f) = open_append(&path)
        {
            let _ = f.write_all(text.as_bytes());
        }
        let dropped = state.as_ref().map_or(0, |s| s.dropped());
        drop(state);
        // A panic after `finish()` must not leave an end.json that says the run was clean.
        if self.finished.load(Ordering::SeqCst) {
            self.write_end("panicked", false, dropped);
        }
    }
}

/// Installs the hook once for `inner`; the hook holds a weak reference so it never keeps the
/// recorder alive.
pub(crate) fn install(inner: &Arc<Inner>) {
    if inner.hook_installed.swap(true, Ordering::SeqCst) {
        return;
    }
    let weak: Weak<Inner> = Arc::downgrade(inner);
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        if let Some(inner) = weak.upgrade() {
            inner.write_panic(info);
        }
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_size_cap_stops_appending() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("panic.txt");
        assert!(under_cap(&p, 10));
        std::fs::write(&p, "123456789").unwrap();
        assert!(under_cap(&p, 10));
        std::fs::write(&p, "1234567890").unwrap();
        assert!(!under_cap(&p, 10));
    }
}
