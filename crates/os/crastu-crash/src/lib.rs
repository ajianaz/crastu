//! Fatal-signal handler for the flight recorder: when the process dies from SIGSEGV, SIGBUS,
//! SIGILL, SIGFPE or SIGABRT, `crash.txt` appears in the current session folder, and the process
//! then dies by the same signal with its default action, so the system's own crash report
//! (macOS ReportCrash, a Linux core dump) still happens.
//!
//! ```text
//! Crastu crash report
//! signal: SIGSEGV (11)
//! code: 1
//! address: 0x10
//! pid: 4242
//! session: 20261112T101500Z-4242
//! time: 2026-11-12T10:15:03Z (unix 1794478503)
//! ```
//!
//! The file holds those fixed fields only: never a message, an argument, an environment
//! variable or memory contents, so it cannot carry user content or a key. `code` is the
//! signal's `si_code` (for SIGSEGV 1 is an unmapped address, 2 a protection fault). `address` is
//! the faulting address when the code says the kernel raised the signal, and `none` for SIGABRT
//! and for a signal sent with `kill` or `raise` on Linux; macOS gives a sent signal a fault-like
//! code with address 0, and it is written as it comes. A crash leaves no `end.json` (the run "ended badly"); `crash.txt` is
//! the reason.
//!
//! # Why this crate allows `unsafe`
//!
//! `unsafe` is denied in the workspace. Installing a signal handler, an alternate signal stack
//! and writing a file from inside the handler cannot be done in safe Rust: they are libc calls
//! (`sigaction`, `sigaltstack`, `open`, `write`, `close`, `getpid`, `clock_gettime`, `raise`).
//! So this crate opts in, in one module (`sys`, Unix only) with each block commented, and it is
//! the only place the recorder's crash path uses `unsafe` (`crastu-recorder` stays safe; the
//! binaries call [`install`] with the recorder's session folder). The report formatting and the
//! path preparation are safe code with unit tests.
//!
//! # What the handler may do
//!
//! A signal handler can interrupt any code, including `malloc` holding its lock, so it must be
//! async-signal-safe: no allocation, no locks, no `std::fmt`, no `std::io`, no panic. Everything
//! it needs is prepared by [`install`]: the absolute path of `crash.txt` as a NUL-terminated
//! buffer and the session id, kept in a leaked allocation the handler only reads. The report is
//! formatted into a fixed 1 KB stack buffer with integer-to-text helpers, and written with
//! `open(O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW, 0600)`, `write` and `close`, so an existing
//! file or a symlink is never written through.
//!
//! Only the first fatal signal writes: an atomic flag sends any later one (a second thread, or a
//! fault inside the handler) straight to the default action. After writing, the handler calls
//! the handler that was installed before it, if that was a function, then restores the default
//! action and raises the signal again. So a fatal signal always ends the process: a handler
//! installed earlier that would have recovered from a fault (some runtimes use SIGSEGV on
//! purpose) is still called, but its return no longer resumes the program. Nothing in Crastu
//! does that today; such a library would have to be loaded before `install` and checked first.
//!
//! # With Rust's own handlers
//!
//! - **Panics** do not raise signals while unwinding (`panic = "unwind"`), so `panic.txt` from
//!   the recorder's panic hook stays the report for them. A panic that aborts (a panic while
//!   panicking, a panic across `extern "C"`) raises SIGABRT, so both files are written:
//!   `panic.txt` from the hook first, then `crash.txt` with SIGABRT. `std::process::abort` alone
//!   writes `crash.txt` only.
//! - **Stack overflow.** At start-up std installs a SIGSEGV and SIGBUS handler that recognises a
//!   hit on a thread's guard page, prints `thread '...' has overflowed its stack` and aborts;
//!   every thread std starts gets an alternate signal stack for it. [`install`] saves that
//!   handler and chains to it, so an overflow writes `crash.txt` (SIGSEGV or SIGBUS, on the
//!   alternate stack), then std prints its line and aborts, and that second signal (SIGABRT) goes
//!   straight to the default action. The process dies by SIGABRT, as without this crate.
//!   [`install`] gives the calling thread an alternate stack if it has none; a thread started by
//!   C code without one still gets `crash.txt` for an ordinary fault, but a stack overflow there
//!   kills it before any handler can run.
//!
//! Unix only (macOS, Linux, and it builds for iOS and Android). On Windows [`install`] returns
//! [`InstallError::Unsupported`]; a structured-exception handler comes later.

use std::fmt;
use std::path::Path;

mod paths;
mod report;
#[cfg(unix)]
#[allow(
    unsafe_code,
    reason = "signal handler, alternate stack and raw file writes are libc calls, each one commented"
)]
mod sys;

/// The file written into the session folder.
pub const CRASH_FILE: &str = "crash.txt";

/// Why [`install`] did nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallError {
    /// Not a Unix system (Windows comes later).
    Unsupported,
    /// The handler is already installed in this process; the first session folder stays.
    AlreadyInstalled,
    /// The folder cannot be made absolute or holds a NUL byte.
    BadPath,
    /// The path of `crash.txt` would be 1024 bytes or longer.
    PathTooLong,
    /// A system call failed while installing.
    System { call: &'static str, errno: i32 },
}

impl fmt::Display for InstallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InstallError::Unsupported => f.write_str("not supported on this system"),
            InstallError::AlreadyInstalled => f.write_str("already installed in this process"),
            InstallError::BadPath => f.write_str("the session folder path cannot be used"),
            InstallError::PathTooLong => f.write_str("the session folder path is too long"),
            InstallError::System { call, errno } => write!(f, "{call} failed (errno {errno})"),
        }
    }
}

impl std::error::Error for InstallError {}

/// Installs the handler so a fatal signal writes `<session_dir>/crash.txt`. Call it once, early,
/// from the thread that runs the work (it gets an alternate signal stack if it has none), with
/// the recorder's session folder. A later call returns [`InstallError::AlreadyInstalled`].
pub fn install(session_dir: &Path) -> Result<(), InstallError> {
    let path = paths::crash_path(session_dir)?;
    let session = paths::session_id(session_dir);
    #[cfg(unix)]
    {
        sys::install(path, session)
    }
    #[cfg(not(unix))]
    {
        let _ = (path, session);
        Err(InstallError::Unsupported)
    }
}
