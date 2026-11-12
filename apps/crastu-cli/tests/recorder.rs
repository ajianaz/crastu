//! The flight recorder through the real binary: a recorded command leaves a session folder with
//! `session.json` and `events.jsonl`, and `end.json` when it exits (including `host`, which ends
//! with `process::exit` on `stop`); a command that is not recorded leaves nothing; a host that
//! dies by a fatal signal leaves `crash.txt` and no `end.json`, and still dies by that signal.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const EXE: &str = env!("CARGO_BIN_EXE_crastu-cli");

fn cmd(tmp: &Path) -> Command {
    let mut c = Command::new(EXE);
    c.env("CRASTU_SOCKET", tmp.join("crastu.sock"))
        .env("CRASTU_DATA", tmp.join("data"))
        .env("CRASTU_LOG_DIR", tmp.join("logs"))
        .env("CRASTU_OFFLINE", "1");
    c
}

/// Kills the child when the test ends, so a failed assertion cannot leak a running host.
struct KillOnDrop(std::process::Child);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn sessions(tmp: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(tmp.join("logs/crastu-cli")) else {
        return Vec::new();
    };
    rd.flatten().map(|e| e.path()).collect()
}

#[test]
fn a_recorded_command_leaves_a_clean_session() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(
        cmd(tmp.path())
            .args(["models", "list"])
            .output()
            .unwrap()
            .status
            .success()
    );
    let s = sessions(tmp.path());
    assert_eq!(s.len(), 1, "{s:?}");
    let session: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(s[0].join("session.json")).unwrap()).unwrap();
    assert_eq!(session["app"], "crastu-cli");
    assert!(s[0].join("end.json").exists());
    let events = std::fs::read_to_string(s[0].join("events.jsonl")).unwrap();
    assert!(events.contains("command models"), "{events}");
}

#[test]
fn commands_that_are_not_recorded_leave_no_folder() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(
        cmd(tmp.path())
            .arg("version")
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(sessions(tmp.path()).is_empty());
}

/// Starts `crastu-cli host` and waits for its socket.
fn start_host(tmp: &Path) -> KillOnDrop {
    let host = KillOnDrop(
        cmd(tmp)
            .arg("host")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let start = Instant::now();
    while !tmp.join("crastu.sock").exists() {
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "the host did not start"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    host
}

#[test]
fn a_host_stopped_by_request_still_writes_end_json() {
    let tmp = tempfile::tempdir().unwrap();
    let mut host = start_host(tmp.path());
    // While it runs there is a folder without end.json.
    let s = sessions(tmp.path());
    assert_eq!(s.len(), 1, "{s:?}");
    assert!(!s[0].join("end.json").exists());
    assert!(cmd(tmp.path()).arg("stop").status().unwrap().success());
    assert!(host.0.wait().unwrap().success());
    assert!(s[0].join("end.json").exists());
}

/// The issue's "forced SIGSEGV in the host": sent from outside, so it is the handler's re-raise
/// that ends the process. SIGSEGV is 11 and SIGABRT 6 on macOS and Linux.
#[cfg(unix)]
#[test]
fn a_host_killed_by_a_fatal_signal_leaves_crash_txt() {
    use std::os::unix::process::ExitStatusExt;
    for (flag, number, name) in [("-SEGV", 11, "SIGSEGV"), ("-ABRT", 6, "SIGABRT")] {
        let tmp = tempfile::tempdir().unwrap();
        let mut host = start_host(tmp.path());
        let pid = host.0.id().to_string();
        assert!(
            Command::new("kill")
                .args([flag, &pid])
                .status()
                .unwrap()
                .success()
        );
        // Without the handler std swallows a sent SIGSEGV and the host lives on: fail, not hang.
        let start = Instant::now();
        let status = loop {
            if let Some(status) = host.0.try_wait().unwrap() {
                break status;
            }
            assert!(
                start.elapsed() < Duration::from_secs(20),
                "{name}: the host did not die"
            );
            std::thread::sleep(Duration::from_millis(50));
        };
        assert_eq!(status.signal(), Some(number), "{name}: {status:?}");
        let s = sessions(tmp.path());
        assert_eq!(s.len(), 1, "{s:?}");
        let crash = std::fs::read_to_string(s[0].join("crash.txt")).unwrap();
        assert!(
            crash.contains(&format!("signal: {name} ({number})")),
            "{crash}"
        );
        assert!(crash.contains(&format!("pid: {pid}\n")), "{crash}");
        assert!(!s[0].join("end.json").exists());
    }
}
