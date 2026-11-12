//! The Unix signal handler. Every `unsafe` block in the crate is here, each one a libc call (or
//! a read of a pointer the kernel or this module handed over) with its reason next to it.
//!
//! Async-signal safety: the handler only loads atomics, reads the leaked [`State`], formats into
//! a stack buffer with the safe code in `report`, and calls `getpid`, `clock_gettime`, `open`,
//! `write`, `close`, `sigaction` and `raise`, all on POSIX's list of async-signal-safe functions.
//! It never allocates, locks, panics or touches `std::io` or `std::fmt`.

use std::ffi::{c_int, c_uint, c_void};
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::{mem, ptr};

use crate::InstallError;
use crate::report::{Fault, Report, write_report};

/// The signals handled, with the names written into `crash.txt`. Numbers differ between
/// systems (SIGBUS is 10 on macOS and 7 on Linux), so they come from libc.
pub(crate) const SIGNALS: [(c_int, &[u8]); 5] = [
    (libc::SIGSEGV, b"SIGSEGV"),
    (libc::SIGBUS, b"SIGBUS"),
    (libc::SIGILL, b"SIGILL"),
    (libc::SIGFPE, b"SIGFPE"),
    (libc::SIGABRT, b"SIGABRT"),
];

/// The alternate signal stack, when this module has to install one: big enough for the report
/// buffer and a chained handler (Rust's own prints a line from it).
const ALT_STACK_MIN: usize = 64 * 1024;

/// Everything the handler reads. Built once, leaked, never changed or freed.
struct State {
    path: Box<[u8]>,
    session: Box<[u8]>,
    /// The actions that were in place before ours, in [`SIGNALS`] order.
    previous: [libc::sigaction; SIGNALS.len()],
}

static STATE: AtomicPtr<State> = AtomicPtr::new(ptr::null_mut());
static INSTALLED: AtomicBool = AtomicBool::new(false);
/// Set by the first fatal signal; any later one (another thread, or a fault inside the
/// handler) goes straight to the default action.
static ENTERED: AtomicBool = AtomicBool::new(false);

type InfoHandler = extern "C" fn(c_int, *mut libc::siginfo_t, *mut c_void);
type PlainHandler = extern "C" fn(c_int);

fn system(call: &'static str) -> InstallError {
    InstallError::System {
        call,
        errno: std::io::Error::last_os_error().raw_os_error().unwrap_or(0),
    }
}

fn zeroed_action() -> libc::sigaction {
    // SAFETY: `sigaction` is a plain C struct; all zeroes is SIG_DFL with an empty mask and no
    // flags, a valid value.
    unsafe { mem::zeroed() }
}

/// Installs the handler for every signal in [`SIGNALS`]. Once per process.
pub(crate) fn install(path: Box<[u8]>, session: Box<[u8]>) -> Result<(), InstallError> {
    if INSTALLED.swap(true, Ordering::SeqCst) {
        return Err(InstallError::AlreadyInstalled);
    }
    alt_stack()?;
    let mut previous = [zeroed_action(); SIGNALS.len()];
    for (slot, (sig, _)) in previous.iter_mut().zip(SIGNALS) {
        // SAFETY: a null new action only reads the current one into `slot`, which is valid.
        if unsafe { libc::sigaction(sig, ptr::null(), slot) } != 0 {
            return Err(system("sigaction"));
        }
    }
    let state: &'static mut State = Box::leak(Box::new(State {
        path,
        session,
        previous,
    }));
    STATE.store(state, Ordering::Release);
    let handler: InfoHandler = on_fatal;
    for (sig, _) in SIGNALS {
        let mut action = zeroed_action();
        action.sa_sigaction = handler as libc::sighandler_t;
        action.sa_flags = libc::SA_SIGINFO | libc::SA_ONSTACK;
        // SAFETY: `action` is a valid struct and `on_fatal` has the SA_SIGINFO signature.
        let set = unsafe {
            libc::sigemptyset(&mut action.sa_mask);
            libc::sigaction(sig, &action, ptr::null_mut())
        };
        if set != 0 {
            return Err(system("sigaction"));
        }
    }
    Ok(())
}

/// Gives the calling thread an alternate signal stack unless it has one, so a stack overflow
/// can still run the handler. Threads started by Rust's std already have one (std installs it
/// for its own overflow message); a thread started by C code may not. The memory is leaked on
/// purpose: the kernel may use it until the thread ends.
fn alt_stack() -> Result<(), InstallError> {
    // SAFETY: `stack_t` is a plain C struct; zeroes are a valid value to be overwritten.
    let mut current: libc::stack_t = unsafe { mem::zeroed() };
    // SAFETY: a null new stack only reads the current one into `current`.
    if unsafe { libc::sigaltstack(ptr::null(), &mut current) } != 0 {
        return Err(system("sigaltstack"));
    }
    if current.ss_flags & libc::SS_DISABLE == 0 {
        return Ok(());
    }
    let size = ALT_STACK_MIN.max(libc::SIGSTKSZ);
    let memory: &'static mut [u8] = Box::leak(vec![0u8; size].into_boxed_slice());
    // SAFETY: as above.
    let mut stack: libc::stack_t = unsafe { mem::zeroed() };
    stack.ss_sp = memory.as_mut_ptr().cast();
    stack.ss_size = size;
    stack.ss_flags = 0;
    // SAFETY: `stack` points at `size` bytes that live for the rest of the process.
    if unsafe { libc::sigaltstack(&stack, ptr::null_mut()) } != 0 {
        return Err(system("sigaltstack"));
    }
    Ok(())
}

/// The handler. Writes `crash.txt` once, passes the signal to the handler that was there before
/// (Rust's stack-overflow reporter, for one), then dies by the signal with its default action.
extern "C" fn on_fatal(sig: c_int, info: *mut libc::siginfo_t, ctx: *mut c_void) {
    if !ENTERED.swap(true, Ordering::SeqCst) {
        // SAFETY: STATE is null or a leaked `State` that is never changed or freed.
        if let Some(state) = unsafe { STATE.load(Ordering::Acquire).as_ref() } {
            write_crash(state, sig, info);
            if let Some(prev) = SIGNALS
                .iter()
                .position(|(s, _)| *s == sig)
                .and_then(|i| state.previous.get(i))
            {
                call_previous(prev, sig, info, ctx);
            }
        }
    }
    die(sig);
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
    Some(ts.tv_sec)
}

/// Writes `crash.txt`. `O_EXCL` and `O_NOFOLLOW`: an existing file or a symlink in its place is
/// left alone (the report is then lost, never written through somewhere else).
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

    #[test]
    fn the_clock_reads() {
        assert!(now().is_some_and(|s| s > 1_700_000_000));
    }
}
