//! The panic hook, tested in a child process: the test binary runs itself with an environment
//! variable that makes the `child_*` test panic on purpose.

use std::fs;
use std::path::Path;
use std::process::Command;

use crastu_recorder::{JobEvent, JobState, Outcome, Recorder};

const CHILD_ROOT: &str = "CRASTU_RECORDER_TEST_ROOT";
const CHILD_MODE: &str = "CRASTU_RECORDER_TEST_MODE";

fn run_child(root: &Path, mode: &str, test: &str) -> std::process::Output {
    Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture", "--test-threads=1"])
        .env(CHILD_ROOT, root)
        .env(CHILD_MODE, mode)
        .output()
        .unwrap()
}

fn only_session(root: &Path) -> std::path::PathBuf {
    let app = root.join("child");
    let mut v: Vec<_> = fs::read_dir(app)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .collect();
    assert_eq!(v.len(), 1, "{v:?}");
    v.remove(0)
}

/// Runs only inside the child process.
#[test]
fn child_panics() {
    let (Some(root), Some(mode)) = (std::env::var_os(CHILD_ROOT), std::env::var_os(CHILD_MODE))
    else {
        return;
    };
    let rec = Recorder::start("child", "9.9", Some(Path::new(&root)));
    rec.install_panic_hook();
    for i in 0..150 {
        rec.info("child", &format!("event number {i}"));
    }
    rec.job_event(&JobEvent {
        kind: "transcribe",
        state: JobState::Started,
        duration_ms: None,
    });
    match mode.to_str() {
        Some("clean") => {
            rec.finish(Outcome::Ok);
            return;
        }
        Some("caught") => {
            // A panic that a thread join catches, and the process still exits normally.
            let h = std::thread::spawn(|| panic!("worker failed"));
            assert!(h.join().is_err());
            rec.finish(Outcome::Ok);
            return;
        }
        Some("after_finish") => rec.finish(Outcome::Ok),
        _ => {}
    }
    // A key in a panic message must be redacted as well.
    panic!("boom sk-abcdefghij1234567890 detail");
}

#[test]
fn a_panic_leaves_panic_txt_with_the_reason_and_the_last_events() {
    let root = tempfile::tempdir().unwrap();
    let out = run_child(root.path(), "panic", "child_panics");
    assert!(!out.status.success(), "the child must have panicked");
    // The previous hook still ran: the default message is on stderr.
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("boom"),
        "chained hook"
    );

    let dir = only_session(root.path());
    let txt = fs::read_to_string(dir.join("panic.txt")).unwrap();
    assert!(txt.contains("message: boom [redacted] detail"), "{txt}");
    assert!(
        txt.contains("location: ") && txt.contains("panic_hook.rs"),
        "{txt}"
    );
    assert!(txt.contains("thread: "), "{txt}");
    assert!(txt.contains("backtrace:"), "{txt}");
    // The last events, newest included, oldest of the 150 cut off at 100.
    assert!(txt.contains("last events (100):"), "{txt}");
    assert!(
        txt.contains("event number 149") && txt.contains("\"job\":\"transcribe\""),
        "{txt}"
    );
    assert!(!txt.contains("event number 40\""), "{txt}");
    assert!(!txt.contains("sk-abcdefghij"), "{txt}");
    // It ended badly: no end.json, and the events file survived.
    assert!(!dir.join("end.json").exists());
    assert_eq!(
        fs::read_to_string(dir.join("events.jsonl"))
            .unwrap()
            .lines()
            .count(),
        151
    );
}

#[test]
fn a_clean_child_has_end_json_and_no_panic_txt() {
    let root = tempfile::tempdir().unwrap();
    let out = run_child(root.path(), "clean", "child_panics");
    assert!(out.status.success());
    let dir = only_session(root.path());
    assert!(dir.join("end.json").exists());
    assert!(!dir.join("panic.txt").exists());
}

fn end_json(dir: &Path) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(dir.join("end.json")).unwrap()).unwrap()
}

#[test]
fn a_clean_child_records_status_ok() {
    let root = tempfile::tempdir().unwrap();
    run_child(root.path(), "clean", "child_panics");
    let e = end_json(&only_session(root.path()));
    assert_eq!(
        (e["status"].as_str(), e["clean"].as_bool()),
        (Some("ok"), Some(true))
    );
    assert_eq!(e["panics"], 0);
}

#[test]
fn a_caught_panic_in_a_thread_does_not_prevent_end_json() {
    let root = tempfile::tempdir().unwrap();
    let out = run_child(root.path(), "caught", "child_panics");
    assert!(out.status.success());
    let dir = only_session(root.path());
    let e = end_json(&dir);
    assert_eq!(
        (e["status"].as_str(), e["clean"].as_bool()),
        (Some("ok"), Some(true))
    );
    assert_eq!(e["panics"], 1);
    let txt = fs::read_to_string(dir.join("panic.txt")).unwrap();
    assert!(txt.contains("message: worker failed"), "{txt}");
}

#[test]
fn a_panic_after_finish_does_not_leave_a_clean_end_json() {
    let root = tempfile::tempdir().unwrap();
    let out = run_child(root.path(), "after_finish", "child_panics");
    assert!(!out.status.success());
    let dir = only_session(root.path());
    let e = end_json(&dir);
    assert_eq!(
        (e["status"].as_str(), e["clean"].as_bool()),
        (Some("panicked"), Some(false))
    );
    assert_eq!(e["panics"], 1);
    assert!(dir.join("panic.txt").exists());
}
