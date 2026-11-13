//! Installing the handler: the alternate signal stack, the saved actions and ours. Runs in normal
//! code (not in a signal handler), so it may allocate.
//!
//! An install is all or nothing: if any step fails, the actions already replaced are put back,
//! the handler state is cleared and a later call may try again. Only a full success marks the
//! process as installed.

use std::ffi::{c_int, c_void};
use std::ptr;
use std::sync::atomic::{AtomicU8, Ordering};

use super::{InfoHandler, SIGNALS, STATE, State, on_fatal, zeroed_action};
use crate::InstallError;

const IDLE: u8 = 0;
const BUSY: u8 = 1;
const DONE: u8 = 2;
static INSTALL: AtomicU8 = AtomicU8::new(IDLE);

/// The alternate signal stack, when one has to be made: big enough for the report buffer and a
/// chained handler (Rust's own prints a line from it).
const ALT_STACK_MIN: usize = 64 * 1024;

/// The system calls an install makes, so the all-or-nothing logic can be tested without
/// touching the test process's real signal handlers.
pub(super) trait Ops {
    fn alt_stack(&mut self) -> Result<(), InstallError>;
    fn get(&mut self, sig: c_int) -> Result<libc::sigaction, InstallError>;
    fn set(&mut self, sig: c_int, action: &libc::sigaction) -> Result<(), InstallError>;
}

fn system(call: &'static str) -> InstallError {
    InstallError::System {
        call,
        errno: std::io::Error::last_os_error().raw_os_error().unwrap_or(0),
    }
}

struct Real;

impl Ops for Real {
    fn alt_stack(&mut self) -> Result<(), InstallError> {
        alt_stack()
    }

    fn get(&mut self, sig: c_int) -> Result<libc::sigaction, InstallError> {
        let mut current = zeroed_action();
        // SAFETY: a null new action only reads the current one into `current`, which is valid.
        if unsafe { libc::sigaction(sig, ptr::null(), &mut current) } != 0 {
            return Err(system("sigaction"));
        }
        Ok(current)
    }

    fn set(&mut self, sig: c_int, action: &libc::sigaction) -> Result<(), InstallError> {
        // SAFETY: `action` is a valid struct; ours has the SA_SIGINFO signature of `on_fatal`,
        // and a saved one came from `sigaction` itself.
        if unsafe { libc::sigaction(sig, action, ptr::null_mut()) } != 0 {
            return Err(system("sigaction"));
        }
        Ok(())
    }
}

/// Installs the handler for every signal in [`SIGNALS`]. Once per process, after a success.
pub(crate) fn install(path: Box<[u8]>, session: Box<[u8]>) -> Result<(), InstallError> {
    install_with(path, session, &mut Real)
}

pub(super) fn install_with(
    path: Box<[u8]>,
    session: Box<[u8]>,
    ops: &mut dyn Ops,
) -> Result<(), InstallError> {
    if INSTALL
        .compare_exchange(IDLE, BUSY, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(InstallError::AlreadyInstalled);
    }
    let result = try_install(path, session, ops);
    INSTALL.store(if result.is_ok() { DONE } else { IDLE }, Ordering::SeqCst);
    result
}

fn our_action() -> libc::sigaction {
    let handler: InfoHandler = on_fatal;
    let mut action = zeroed_action();
    action.sa_sigaction = handler as libc::sighandler_t;
    action.sa_flags = libc::SA_SIGINFO | libc::SA_ONSTACK;
    // SAFETY: `sa_mask` is a valid `sigset_t` to initialise.
    unsafe { libc::sigemptyset(&mut action.sa_mask) };
    action
}

fn try_install(path: Box<[u8]>, session: Box<[u8]>, ops: &mut dyn Ops) -> Result<(), InstallError> {
    ops.alt_stack()?;
    let mut previous = [zeroed_action(); SIGNALS.len()];
    for (slot, (sig, _)) in previous.iter_mut().zip(SIGNALS) {
        *slot = ops.get(sig)?;
    }
    let state: &'static mut State = Box::leak(Box::new(State {
        path,
        session,
        previous,
    }));
    STATE.store(state, Ordering::Release);
    let ours = our_action();
    for (i, (sig, _)) in SIGNALS.iter().enumerate() {
        if let Err(e) = ops.set(*sig, &ours) {
            // Put back what was replaced, newest first, and forget the state (it stays leaked:
            // a handler may have read it already).
            for ((s, _), prev) in SIGNALS.iter().zip(&previous).take(i).rev() {
                let _ = ops.set(*s, prev);
            }
            STATE.store(ptr::null_mut(), Ordering::Release);
            return Err(e);
        }
    }
    Ok(())
}

/// Gives the calling thread an alternate signal stack unless it has one, so a stack overflow
/// can still run the handler. Threads started by Rust's std already have one (std installs it
/// for its own overflow message); a thread started by C code may not.
fn alt_stack() -> Result<(), InstallError> {
    // SAFETY: `stack_t` is a plain C struct; zeroes are a valid value to be overwritten.
    let mut current: libc::stack_t = unsafe { std::mem::zeroed() };
    // SAFETY: a null new stack only reads the current one into `current`.
    if unsafe { libc::sigaltstack(ptr::null(), &mut current) } != 0 {
        return Err(system("sigaltstack"));
    }
    if current.ss_flags & libc::SS_DISABLE == 0 {
        return Ok(());
    }
    let size = ALT_STACK_MIN.max(libc::SIGSTKSZ);
    let (sp, size) = map_stack(size)?;
    // SAFETY: as above.
    let mut stack: libc::stack_t = unsafe { std::mem::zeroed() };
    stack.ss_sp = sp;
    stack.ss_size = size;
    stack.ss_flags = 0;
    // SAFETY: `stack` points at `size` mapped bytes that stay mapped for the rest of the process.
    if unsafe { libc::sigaltstack(&stack, ptr::null_mut()) } != 0 {
        return Err(system("sigaltstack"));
    }
    Ok(())
}

/// Maps a stack of at least `size` bytes (rounded up to whole pages) with one inaccessible guard
/// page below it, as std does for its own alternate stacks: a handler that runs off the end
/// faults on the guard instead of writing over other memory. Never unmapped: the kernel may use
/// it until the thread ends. Returns the usable start and size.
fn map_stack(size: usize) -> Result<(*mut c_void, usize), InstallError> {
    // SAFETY: `sysconf` has no preconditions.
    let page = usize::try_from(unsafe { libc::sysconf(libc::_SC_PAGESIZE) })
        .ok()
        .filter(|p| *p > 0)
        .unwrap_or(4096);
    let size = size.div_ceil(page).saturating_mul(page);
    let total = size.saturating_add(page);
    // SAFETY: an anonymous private mapping at an address of the kernel's choosing.
    let base = unsafe {
        libc::mmap(
            ptr::null_mut(),
            total,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANON,
            -1,
            0,
        )
    };
    if base == libc::MAP_FAILED {
        return Err(system("mmap"));
    }
    // SAFETY: the first page of the mapping made above.
    if unsafe { libc::mprotect(base, page, libc::PROT_NONE) } != 0 {
        let err = system("mprotect");
        // SAFETY: unmaps exactly the mapping made above, which nothing uses yet.
        unsafe { libc::munmap(base, total) };
        return Err(err);
    }
    Ok((base.cast::<u8>().wrapping_add(page).cast(), size))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Records every call; `fail_alt` and `fail_set_at` (the n-th set of our handler) fail.
    struct Fake {
        fail_alt: bool,
        fail_set_at: Option<usize>,
        ours_set: usize,
        calls: Vec<(c_int, libc::sighandler_t)>,
    }

    impl Fake {
        fn new(fail_alt: bool, fail_set_at: Option<usize>) -> Fake {
            Fake {
                fail_alt,
                fail_set_at,
                ours_set: 0,
                calls: Vec::new(),
            }
        }
    }

    /// The fake "previous" action for a signal: an address no real handler has.
    fn previous_of(sig: c_int) -> libc::sighandler_t {
        1000 + usize::try_from(sig).unwrap()
    }

    impl Ops for Fake {
        fn alt_stack(&mut self) -> Result<(), InstallError> {
            if self.fail_alt {
                return Err(InstallError::System {
                    call: "sigaltstack",
                    errno: 12,
                });
            }
            Ok(())
        }

        fn get(&mut self, sig: c_int) -> Result<libc::sigaction, InstallError> {
            let mut a = zeroed_action();
            a.sa_sigaction = previous_of(sig);
            Ok(a)
        }

        fn set(&mut self, sig: c_int, action: &libc::sigaction) -> Result<(), InstallError> {
            if action.sa_sigaction == our_action().sa_sigaction {
                if self.fail_set_at == Some(self.ours_set) {
                    return Err(InstallError::System {
                        call: "sigaction",
                        errno: 22,
                    });
                }
                self.ours_set += 1;
            }
            self.calls.push((sig, action.sa_sigaction));
            Ok(())
        }
    }

    fn bufs() -> (Box<[u8]>, Box<[u8]>) {
        (Box::from(&b"/x/crash.txt\0"[..]), Box::from(&b"s"[..]))
    }

    /// One test, because it walks the process-wide install state through its steps. No real
    /// handler is installed: every call goes to the fake.
    #[test]
    fn a_failed_install_is_rolled_back_and_can_be_retried() {
        let ours = our_action().sa_sigaction;
        // The alternate stack fails: nothing is touched.
        let mut f = Fake::new(true, None);
        let (p, s) = bufs();
        assert!(matches!(
            install_with(p, s, &mut f),
            Err(InstallError::System {
                call: "sigaltstack",
                ..
            })
        ));
        assert!(f.calls.is_empty());
        assert_eq!(INSTALL.load(Ordering::SeqCst), IDLE);

        // The third signal fails: the first two get their previous actions back, newest first.
        let mut f = Fake::new(false, Some(2));
        let (p, s) = bufs();
        assert!(matches!(
            install_with(p, s, &mut f),
            Err(InstallError::System {
                call: "sigaction",
                ..
            })
        ));
        let (s0, s1) = (SIGNALS[0].0, SIGNALS[1].0);
        assert_eq!(
            f.calls,
            vec![
                (s0, ours),
                (s1, ours),
                (s1, previous_of(s1)),
                (s0, previous_of(s0)),
            ]
        );
        assert!(STATE.load(Ordering::SeqCst).is_null());
        assert_eq!(INSTALL.load(Ordering::SeqCst), IDLE);

        // A retry succeeds and sets every signal; after that, installs are refused.
        let mut f = Fake::new(false, None);
        let (p, s) = bufs();
        assert_eq!(install_with(p, s, &mut f), Ok(()));
        assert_eq!(f.calls.len(), SIGNALS.len());
        assert!(f.calls.iter().all(|(_, h)| *h == ours));
        assert!(!STATE.load(Ordering::SeqCst).is_null());
        let (p, s) = bufs();
        assert_eq!(
            install_with(p, s, &mut Fake::new(false, None)),
            Err(InstallError::AlreadyInstalled)
        );
    }

    #[test]
    fn the_mapped_stack_is_whole_pages_and_writable() {
        let (sp, size) = map_stack(10_000).unwrap();
        assert!(size >= 10_000);
        assert_eq!(sp as usize % 4096, 0);
        let bytes = sp.cast::<u8>();
        // SAFETY: both ends of the usable range belong to the mapping (test only).
        unsafe {
            bytes.write_volatile(1);
            bytes.wrapping_add(size - 1).write_volatile(2);
            assert_eq!(bytes.read_volatile(), 1);
        }
    }
}
