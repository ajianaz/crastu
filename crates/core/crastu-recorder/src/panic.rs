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

use crate::io::open_append;
use crate::time::Utc;
use crate::{Inner, PANIC_FILE, RING_DUMP};
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

impl Inner {
    /// Appends a report for `info` to `panic.txt`, and marks a run that had already finished as
    /// panicked. Never panics; does nothing without a folder.
    fn write_panic(&self, info: &PanicHookInfo<'_>) {
        self.panics.fetch_add(1, Ordering::SeqCst);
        let Some(dir) = &self.dir else { return };
        let thread = std::thread::current();
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown".to_string());
        let mut text = String::new();
        let _ = writeln!(text, "crastu panic at {}", Utc::now().rfc3339());
        let _ = writeln!(text, "thread: {}", thread.name().unwrap_or("<unnamed>"));
        let _ = writeln!(text, "location: {location}");
        let _ = writeln!(text, "message: {}", clean(&payload_text(info), 2000));
        let _ = writeln!(text, "\nbacktrace:\n{}", Backtrace::force_capture());
        // A poisoned lock still holds the events: a panic that poisoned it is the usual reason
        // for the next panic report, and those are the events worth having.
        let state = match self.state.try_lock() {
            Ok(g) => Some(g),
            Err(TryLockError::Poisoned(p)) => Some(p.into_inner()),
            Err(TryLockError::WouldBlock) => None,
        };
        match &state {
            Some(state) => {
                let skip = state.ring.len().saturating_sub(RING_DUMP);
                let _ = writeln!(
                    text,
                    "last events ({}):",
                    state.ring.len().saturating_sub(skip)
                );
                for line in state.ring.iter().skip(skip) {
                    let _ = writeln!(text, "{line}");
                }
            }
            None => {
                let _ = writeln!(text, "last events: unavailable (the event log was in use)");
            }
        }
        let _ = writeln!(text, "---");
        if let Ok(mut f) = open_append(&dir.join(PANIC_FILE)) {
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
