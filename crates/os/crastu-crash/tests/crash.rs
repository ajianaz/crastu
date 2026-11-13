//! The handler in a real process. The test binary runs itself as a child with an environment
//! variable that makes `child_crashes` start a recorder session, install the handler and then
//! die on purpose; the parent checks `crash.txt` and how the child died.
//!
//! `unsafe` appears here only to cause the crashes (an invalid write) and to remove the alternate
//! signal stack std gives its threads, so a thread looks like one that C code started.
//!
//! On macOS each child that dies here is a real crash to the system: ReportCrash writes an
//! `.ips` report to `~/Library/Logs/DiagnosticReports` for every run of these tests (and on
//! Linux a core dump may be written, depending on the system settings). That is the point of
//! re-raising, not a side effect to avoid.
//!
//! `O_NOFOLLOW` has no test of its own: `O_CREAT | O_EXCL` already refuses any symlink in place
//! of `crash.txt`, dangling or not (the symlink test below points at a file that does not
//! exist), so removing `O_NOFOLLOW` alone changes nothing observable. It stays as a second guard.

#![cfg(unix)]

use std::fs;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crastu_crash::{CRASH_FILE, InstallError, install};
use crastu_recorder::{Outcome, Recorder};

const CHILD_ROOT: &str = "CRASTU_CRASH_TEST_ROOT";
const CHILD_MODE: &str = "CRASTU_CRASH_TEST_MODE";
/// Planted in the child's environment and event log; must never reach `crash.txt`.
const SECRET: &str = "sk-crashtestsecret0123456789abcdef";
const CONTENT: &str = "the private words of a transcript";

struct Crash {
    status: std::process::ExitStatus,
    pid: u32,
    session: PathBuf,
    stderr: String,
}

impl Crash {
    fn text(&self) -> String {
        fs::read_to_string(self.session.join(CRASH_FILE)).unwrap_or_else(|e| {
            panic!(
                "no crash.txt ({e}); status {:?}; stderr:\n{}",
                self.status, self.stderr
            )
        })
    }
}

fn run_child(mode: &str) -> (tempfile::TempDir, Crash) {
    let root = tempfile::tempdir().unwrap();
    let child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "child_crashes",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_ROOT, root.path())
        .env(CHILD_MODE, mode)
        .env("CRASTU_TEST_SECRET", SECRET)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let pid = child.id();
    let out = child.wait_with_output().unwrap();
    let app = root.path().join("child");
    let mut sessions: Vec<PathBuf> = fs::read_dir(&app)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .collect();
    assert_eq!(sessions.len(), 1, "{sessions:?}");
    let crash = Crash {
        status: out.status,
        pid,
        session: sessions.remove(0),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    };
    (root, crash)
}

#[allow(
    unsafe_code,
    reason = "test only: the deliberate invalid write the handler must report"
)]
fn write_to_address_0x10() {
    let p = std::ptr::without_provenance_mut::<u8>(0x10);
    // SAFETY: none; this write is meant to fault.
    unsafe { p.write_volatile(1) };
}

#[allow(
    unsafe_code,
    reason = "test only: drops std's alternate signal stack, like a thread C code started"
)]
fn drop_alt_stack() {
    // SAFETY: a zeroed `stack_t` with SS_DISABLE turns the alternate stack off for this thread.
    let mut ss: libc::stack_t = unsafe { std::mem::zeroed() };
    ss.ss_flags = libc::SS_DISABLE;
    // macOS checks the size even when disabling.
    ss.ss_size = libc::SIGSTKSZ;
    let r = unsafe { libc::sigaltstack(&ss, std::ptr::null_mut()) };
    assert_eq!(r, 0, "{}", std::io::Error::last_os_error());
}

fn recurse(n: u64) -> u64 {
    let frame = [n; 64];
    if std::hint::black_box(n) == u64::MAX {
        return 0;
    }
    recurse(n + 1).wrapping_add(std::hint::black_box(&frame)[0])
}

/// Runs only inside the child process.
#[test]
fn child_crashes() {
    let (Some(root), Some(mode)) = (std::env::var_os(CHILD_ROOT), std::env::var_os(CHILD_MODE))
    else {
        return;
    };
    let mode = mode.to_string_lossy().into_owned();
    let rec = Recorder::start("child", "1", Some(Path::new(&root)));
    rec.install_panic_hook();
    rec.info("child", &format!("{CONTENT} {SECRET}"));
    let dir = rec.session_dir().unwrap().to_path_buf();
    if mode == "overflow_own_stack" {
        drop_alt_stack();
    }
    install(&dir).unwrap();
    match mode.as_str() {
        "segv" => write_to_address_0x10(),
        "abort" => std::process::abort(),
        "overflow" | "overflow_own_stack" => {
            recurse(0);
        }
        "killed" => {
            // A SIGSEGV sent from outside: no fault to repeat, so only the re-raise kills it.
            let me = std::process::id().to_string();
            assert!(
                Command::new("kill")
                    .args(["-SEGV", &me])
                    .status()
                    .unwrap()
                    .success()
            );
            std::thread::sleep(std::time::Duration::from_secs(10));
        }
        "existing" => {
            fs::write(dir.join(CRASH_FILE), "earlier").unwrap();
            std::process::abort();
        }
        "symlink" => {
            std::os::unix::fs::symlink(Path::new(&root).join("target"), dir.join(CRASH_FILE))
                .unwrap();
            std::process::abort();
        }
        "two_threads" => {
            // Both threads fault at once; the second must wait for the first one's report.
            let gate = std::sync::Arc::new(std::sync::Barrier::new(2));
            let threads: Vec<_> = (0..2)
                .map(|_| {
                    let gate = gate.clone();
                    std::thread::spawn(move || {
                        gate.wait();
                        write_to_address_0x10();
                    })
                })
                .collect();
            for t in threads {
                let _ = t.join();
            }
        }
        "twice" => {
            assert_eq!(install(&dir), Err(InstallError::AlreadyInstalled));
            rec.finish(Outcome::Ok);
        }
        other => panic!("unknown mode {other}"),
    }
    // Past this point only `twice` (on purpose) or a crash that did not happen: a normal exit.
}

/// The fields every report has, and nothing from the child's content.
fn assert_common(c: &Crash, text: &str) {
    assert!(text.starts_with("Crastu crash report\n"), "{text}");
    assert!(text.contains(&format!("\npid: {}\n", c.pid)), "{text}");
    let id = c.session.file_name().unwrap().to_str().unwrap();
    assert!(text.contains(&format!("\nsession: {id}\n")), "{text}");
    assert!(text.contains("\ntime: 20"), "{text}");
    assert!(!text.contains(SECRET) && !text.contains("sk-"), "{text}");
    assert!(
        !text.contains(CONTENT) && !text.contains("private"),
        "{text}"
    );
    // A crash is not a clean end.
    assert!(!c.session.join("end.json").exists());
    let mode = fs::metadata(c.session.join(CRASH_FILE))
        .unwrap()
        .permissions();
    assert_eq!(
        std::os::unix::fs::PermissionsExt::mode(&mode) & 0o777,
        0o600
    );
}

#[test]
fn an_invalid_write_leaves_crash_txt_and_the_process_dies_by_sigsegv() {
    let (_root, c) = run_child("segv");
    let text = c.text();
    let sig = c.status.signal();
    // An unmapped page is SIGSEGV on Linux and macOS; accept SIGBUS but require consistency.
    assert!(
        sig == Some(libc::SIGSEGV) || sig == Some(libc::SIGBUS),
        "{:?}",
        c.status
    );
    let name = if sig == Some(libc::SIGSEGV) {
        "SIGSEGV"
    } else {
        "SIGBUS"
    };
    assert!(
        text.contains(&format!("\nsignal: {name} ({})\n", sig.unwrap())),
        "{text}"
    );
    assert!(text.contains("\naddress: 0x10\n"), "{text}");
    assert_common(&c, &text);
}

#[test]
fn an_abort_leaves_crash_txt_and_the_process_dies_by_sigabrt() {
    let (_root, c) = run_child("abort");
    assert_eq!(c.status.signal(), Some(libc::SIGABRT), "{:?}", c.status);
    let text = c.text();
    assert!(
        text.contains(&format!("\nsignal: SIGABRT ({})\n", libc::SIGABRT)),
        "{text}"
    );
    assert!(text.contains("\naddress: none\n"), "{text}");
    assert_common(&c, &text);
}

#[test]
fn a_signal_sent_from_outside_is_raised_again() {
    let (_root, c) = run_child("killed");
    assert_eq!(c.status.signal(), Some(libc::SIGSEGV), "{:?}", c.status);
    let text = c.text();
    assert!(
        text.contains(&format!("\nsignal: SIGSEGV ({})\n", libc::SIGSEGV)),
        "{text}"
    );
    // Linux marks a sent signal with a user code, so no address is shown. macOS gives `kill`
    // a fault-like code (2 on macOS 26) with address 0, which is reported as it comes.
    if cfg!(target_os = "linux") {
        assert!(text.contains("\naddress: none\n"), "{text}");
    }
    assert_common(&c, &text);
}

fn assert_overflow(c: &Crash) {
    let text = c.text();
    assert!(
        text.contains("\nsignal: SIGSEGV (") || text.contains("\nsignal: SIGBUS ("),
        "{text}"
    );
    assert_common(c, &text);
    // std's own overflow report still runs after ours, and it aborts.
    assert!(
        c.stderr.contains("has overflowed its stack"),
        "{}",
        c.stderr
    );
    assert_eq!(c.status.signal(), Some(libc::SIGABRT), "{:?}", c.status);
}

#[test]
fn a_stack_overflow_leaves_crash_txt_and_std_still_aborts() {
    let (_root, c) = run_child("overflow");
    assert_overflow(&c);
}

#[test]
fn a_stack_overflow_on_a_thread_without_an_alternate_stack_uses_ours() {
    let (_root, c) = run_child("overflow_own_stack");
    assert_overflow(&c);
}

#[test]
fn an_existing_crash_txt_is_never_overwritten() {
    let (_root, c) = run_child("existing");
    assert_eq!(c.status.signal(), Some(libc::SIGABRT), "{:?}", c.status);
    assert_eq!(c.text(), "earlier");
}

#[test]
fn a_symlink_in_place_of_crash_txt_is_not_followed() {
    let (root, c) = run_child("symlink");
    assert_eq!(c.status.signal(), Some(libc::SIGABRT), "{:?}", c.status);
    assert!(!root.path().join("target").exists());
}

#[test]
fn a_second_install_is_refused() {
    let (_root, c) = run_child("twice");
    assert!(c.status.success(), "{:?}\n{}", c.status, c.stderr);
    assert!(!c.session.join(CRASH_FILE).exists());
    assert!(c.session.join("end.json").exists());
}

/// Two threads faulting together leave one complete report (the later one waits for it). A
/// smoke test: the race is real but not forced, so it cannot prove the wait on its own.
#[test]
fn two_threads_faulting_at_once_leave_one_complete_report() {
    let (_root, c) = run_child("two_threads");
    let sig = c.status.signal();
    assert!(
        sig == Some(libc::SIGSEGV) || sig == Some(libc::SIGBUS),
        "{:?}",
        c.status
    );
    let text = c.text();
    assert!(text.ends_with("crash report has the stack.\n"), "{text}");
    assert_eq!(text.matches("Crastu crash report").count(), 1, "{text}");
    assert_common(&c, &text);
}
