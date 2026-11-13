//! The Unix signal handler. Every `unsafe` block in the crate is here (and in `sys/setup.rs`),
//! each one a libc call (or a read of a pointer the kernel or this module handed over) with its
//! reason next to it.
//!
//! Async-signal safety: the handler only loads and stores atomics, reads the leaked [`State`],
//! formats into a stack buffer with the safe code in `report`, and calls `getpid`,
//! `clock_gettime`, `open`, `write`, `close`, `nanosleep`, `sigaction` and `raise`, all on
//! POSIX's list of async-signal-safe functions. It never allocates, locks, panics or touches
//! `std::io` or `std::fmt`.

use std::ffi::{c_int, c_uint, c_void};
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::{mem, ptr};

use crate::report::{Fault, Report, write_report};

mod setup;

pub(crate) use setup::install;

/// The signals handled, with the names written into `crash.txt`. Numbers differ between
/// systems (SIGBUS is 10 on macOS and 7 on Linux), so they come from libc.
pub(crate) const SIGNALS: [(c_int, &[u8]); 5] = [
    (libc::SIGSEGV, b"SIGSEGV"),
    (libc::SIGBUS, b"SIGBUS"),
    (libc::SIGILL, b"SIGILL"),
    (libc::SIGFPE, b"SIGFPE"),
    (libc::SIGABRT, b"SIGABRT"),
];

/// Everything the handler reads. Built once per install, leaked, never changed or freed.
struct State {
    path: Box<[u8]>,
    session: Box<[u8]>,
    /// The actions that were in place before ours, in [`SIGNALS`] order.
    previous: [libc::sigaction; SIGNALS.len()],
}

static STATE: AtomicPtr<State> = AtomicPtr::new(ptr::null_mut());
/// Set by the first fatal signal. A later one (another thread, or a fault inside the handler)
/// does not write; it waits for [`REPORTED`] and then takes the default action.
static ENTERED: AtomicBool = AtomicBool::new(false);
/// Set once the first signal's `crash.txt` is written (or could not be).
static REPORTED: AtomicBool = AtomicBool::new(false);
/// How long a later signal waits for the first report: this many sleeps of 1 ms.
const WAIT_STEPS: u32 = 500;

type InfoHandler = extern "C" fn(c_int, *mut libc::siginfo_t, *mut c_void);
type PlainHandler = extern "C" fn(c_int);

fn zeroed_action() -> libc::sigaction {
    // SAFETY: `sigaction` is a plain C struct; all zeroes is SIG_DFL with an empty mask and no
    // flags, a valid value.
    unsafe { mem::zeroed() }
}

/// The handler. Writes `crash.txt` once, passes the signal to the handler that was there before
/// (Rust's stack-overflow reporter, for one), then dies by the signal with its default action.
extern "C" fn on_fatal(sig: c_int, info: *mut libc::siginfo_t, ctx: *mut c_void) {
    if ENTERED.swap(true, Ordering::SeqCst) {
        wait_for_report();
    } else {
        // SAFETY: STATE is null or a leaked `State` that is never changed or freed.
        let state = unsafe { STATE.load(Ordering::Acquire).as_ref() };
        if let Some(state) = state {
            write_crash(state, sig, info);
        }
        REPORTED.store(true, Ordering::SeqCst);
        if let Some(prev) = state.and_then(|state| {
            SIGNALS
                .iter()
                .position(|(s, _)| *s == sig)
                .and_then(|i| state.previous.get(i))
        }) {
            call_previous(prev, sig, info, ctx);
        }
    }
    die(sig);
}

/// Lets a later fatal signal wait, up to half a second, for the first one's report, so a second
/// thread that faults while the first is writing does not end the process under it. A fault
/// inside the handler itself waits the full half second, then dies. `nanosleep` is
/// async-signal-safe.
fn wait_for_report() {
    // SAFETY: `timespec` is a plain C struct; the field that matters is set below.
    let mut pause: libc::timespec = unsafe { mem::zeroed() };
    pause.tv_nsec = 1_000_000;
    for _ in 0..WAIT_STEPS {
        if REPORTED.load(Ordering::SeqCst) {
            return;
        }
        // SAFETY: `pause` is valid; the remaining time is not needed.
        unsafe { libc::nanosleep(&pause, ptr::null_mut()) };
    }
}

/// Restores the default action and raises the signal again. The signal is blocked while its
/// handler runs, so it is delivered, with the default action, the moment the handler returns:
/// the process dies by it and the system writes its usual crash report.
fn die(sig: c_int) {
    let action = zeroed_action();
    // SAFETY: a zeroed action is SIG_DFL; `raise` only queues the signal for this thread.
    unsafe {
        libc::sigaction(sig, &action, ptr::null_mut());
        libc::raise(sig);
    }
}

/// Calls the action that was installed before ours, if it was a function. Default and ignore
/// are skipped: [`die`] applies the default action next.
fn call_previous(prev: &libc::sigaction, sig: c_int, info: *mut libc::siginfo_t, ctx: *mut c_void) {
    let raw = prev.sa_sigaction;
    let ours: InfoHandler = on_fatal;
    if raw == libc::SIG_DFL || raw == libc::SIG_IGN || raw == ours as libc::sighandler_t {
        return;
    }
    if prev.sa_flags & libc::SA_SIGINFO != 0 {
        // SAFETY: with SA_SIGINFO the kernel calls `sa_sigaction` with this signature, and the
        // value came from `sigaction` itself.
        let f: InfoHandler = unsafe { mem::transmute::<libc::sighandler_t, InfoHandler>(raw) };
        f(sig, info, ctx);
    } else {
        // SAFETY: without SA_SIGINFO the field holds a one-argument `sa_handler`.
        let f: PlainHandler = unsafe { mem::transmute::<libc::sighandler_t, PlainHandler>(raw) };
        f(sig);
    }
}

/// The faulting address, only when `si_code` says the kernel raised the signal. `kill`, `raise`
/// and `sigqueue` set a user code on Linux (0 or negative) and no meaningful address. macOS
/// defines user codes from 0x10001, but in practice gives `kill` a fault-like code with address
/// 0, which then passes through as `0x0`.
fn fault_address(sig: c_int, info: *const libc::siginfo_t) -> Option<u64> {
    if sig == libc::SIGABRT {
        return None;
    }
    // SAFETY: the kernel passes a valid `siginfo_t` (or null) to an SA_SIGINFO handler.
    let info = unsafe { info.as_ref() }?;
    if info.si_code <= 0 || info.si_code >= 0x10000 {
        return None;
    }
    // SAFETY: for SIGSEGV, SIGBUS, SIGILL and SIGFPE with a kernel code, `si_addr` is the
    // member of the union that the kernel filled in.
    Some(unsafe { info.si_addr() } as usize as u64)
}

fn now() -> Option<i64> {
    // SAFETY: `timespec` is a plain C struct, filled in by `clock_gettime`.
    let mut ts: libc::timespec = unsafe { mem::zeroed() };
    // SAFETY: `ts` is a valid out pointer.
    if unsafe { libc::clock_gettime(libc::CLOCK_REALTIME, &mut ts) } != 0 {
        return None;
    }
    // `time_t` is 32 bits on 32-bit Android and Linux, 64 elsewhere.
    #[allow(clippy::useless_conversion, reason = "time_t is i32 on 32-bit targets")]
    Some(i64::from(ts.tv_sec))
}

/// Writes `crash.txt`. `O_EXCL` and `O_NOFOLLOW`: an existing file or a symlink in its place is
/// left alone (the report is then lost, never written through somewhere else). `O_CREAT |
/// O_EXCL` already refuses a symlink, dangling or not; `O_NOFOLLOW` is a second guard.
fn write_crash(state: &State, sig: c_int, info: *const libc::siginfo_t) {
    let name = SIGNALS
        .iter()
        .find(|(s, _)| *s == sig)
        .map_or(&b"unknown"[..], |(_, n)| *n);
    // SAFETY: `getpid` has no preconditions.
    let pid = unsafe { libc::getpid() };
    let fault = Fault {
        name,
        number: sig,
        // SAFETY: the kernel passes a valid `siginfo_t` (or null) to an SA_SIGINFO handler.
        code: unsafe { info.as_ref() }.map(|i| i.si_code),
        address: fault_address(sig, info),
        pid: u32::try_from(pid).unwrap_or(0),
        unix_secs: now(),
    };
    let mut report = Report::new();
    write_report(&mut report, &fault, &state.session);
    let flags = libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC;
    let mode: c_uint = 0o600;
    // SAFETY: `path` is NUL-terminated (built by `paths::crash_path`) and lives forever.
    let fd = unsafe { libc::open(state.path.as_ptr().cast(), flags, mode) };
    if fd < 0 {
        return;
    }
    let mut rest = report.as_bytes();
    while !rest.is_empty() {
        // SAFETY: `rest` is a valid slice for its length; `fd` is open.
        let n = unsafe { libc::write(fd, rest.as_ptr().cast(), rest.len()) };
        let Ok(n) = usize::try_from(n) else { break };
        if n == 0 {
            break;
        }
        rest = rest.get(n..).unwrap_or(&[]);
    }
    // SAFETY: `fd` was opened above and is closed once.
    unsafe { libc::close(fd) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_signal_names_match_their_numbers() {
        let names: Vec<(c_int, &str)> = SIGNALS
            .iter()
            .map(|(n, s)| (*n, std::str::from_utf8(s).unwrap()))
            .collect();
        assert!(names.contains(&(11, "SIGSEGV")), "{names:?}");
        assert!(names.contains(&(6, "SIGABRT")), "{names:?}");
        assert!(names.contains(&(4, "SIGILL")), "{names:?}");
        assert!(names.contains(&(8, "SIGFPE")), "{names:?}");
        let bus = if cfg!(target_os = "linux") { 7 } else { 10 };
        assert!(names.contains(&(bus, "SIGBUS")), "{names:?}");
    }

    #[test]
    fn no_address_without_a_kernel_fault() {
        assert_eq!(fault_address(libc::SIGSEGV, ptr::null()), None);
        // SAFETY: zeroes are a valid `siginfo_t`.
        let mut info: libc::siginfo_t = unsafe { mem::zeroed() };
        info.si_code = 0;
        assert_eq!(fault_address(libc::SIGSEGV, &info), None);
        info.si_code = 0x10001;
        assert_eq!(fault_address(libc::SIGSEGV, &info), None);
        info.si_code = 1;
        assert_eq!(fault_address(libc::SIGABRT, &info), None);
        assert!(fault_address(libc::SIGSEGV, &info).is_some());
    }

    /// The only test that touches `REPORTED`; no real handler is installed in this process.
    #[test]
    fn a_later_signal_waits_for_the_report_but_not_forever() {
        let start = std::time::Instant::now();
        wait_for_report();
        let waited = start.elapsed();
        assert!(
            waited >= std::time::Duration::from_millis(400),
            "{waited:?}"
        );
        assert!(waited < std::time::Duration::from_secs(5), "{waited:?}");
        REPORTED.store(true, Ordering::SeqCst);
        let start = std::time::Instant::now();
        wait_for_report();
        assert!(start.elapsed() < std::time::Duration::from_millis(50));
    }

    #[test]
    fn the_clock_reads() {
        assert!(now().is_some_and(|s| s > 1_700_000_000));
    }
}
