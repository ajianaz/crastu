//! The panic hook: `panic.txt` with the reason and the last events, then the previous hook.
//!
//! Code here runs while the process is already panicking, and a panic inside a panic hook
//! aborts. So it avoids `unwrap`, indexing and arithmetic that can overflow, uses `try_lock`
//! (the panicking thread may hold the state lock), and ignores every I/O error.

use std::backtrace::Backtrace;
use std::fmt::Write as _;
use std::io::Write as _;
use std::panic::{self, AssertUnwindSafe, PanicHookInfo};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Weak};

use crate::redact::clean;
use crate::time::Utc;
use crate::{Inner, PANIC_FILE, RING_DUMP};

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
    /// Appends a report for `info` to `panic.txt`. Never panics; does nothing without a folder.
    fn write_panic(&self, info: &PanicHookInfo<'_>) {
        // Set first: from here on the run did not end cleanly, whatever else fails.
        self.panicked.store(true, Ordering::SeqCst);
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
        match self.state.try_lock() {
            Ok(state) => {
                let skip = state.ring.len().saturating_sub(RING_DUMP);
                let _ = writeln!(text, "last events ({}):", state.ring.len() - skip);
                for line in state.ring.iter().skip(skip) {
                    let _ = writeln!(text, "{line}");
                }
            }
            Err(_) => {
                let _ = writeln!(text, "last events: unavailable (the event log was in use)");
            }
        }
        let _ = writeln!(text, "---");
        if let Ok(mut f) = crate::open_append(&dir.join(PANIC_FILE)) {
            let _ = f.write_all(text.as_bytes());
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
        let _ = panic::catch_unwind(AssertUnwindSafe(|| {
            if let Some(inner) = weak.upgrade() {
                inner.write_panic(info);
            }
        }));
        previous(info);
    }));
}
