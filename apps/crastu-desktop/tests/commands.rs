//! The desktop binary answers the headless commands without opening a window: `crastu version`
//! and `crastu mcp`, the command the design shows for `claude mcp add`. The `mcp` shim has to
//! start the host by running this same binary with `host`.

#![cfg(unix)]

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

const EXE: &str = env!("CARGO_BIN_EXE_crastu");

#[test]
fn version_is_printed_without_a_window() {
    let out = Command::new(EXE).arg("version").output().unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.starts_with("crastu "), "{text}");
}

/// Kills the shim and stops the host it started, also when an assertion fails halfway. The host
/// may still be starting when the test fails, so the stop is tried a few times.
struct Cleanup {
    child: std::process::Child,
    socket: std::path::PathBuf,
    data: std::path::PathBuf,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        for _ in 0..5 {
            let stopped = Command::new(EXE)
                .arg("stop")
                .env("CRASTU_SOCKET", &self.socket)
                .env("CRASTU_DATA", &self.data)
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|s| s.success());
            if stopped {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(300));
        }
    }
}

#[test]
fn mcp_lists_the_tools_over_stdio() {
    let tmp = tempfile::tempdir().unwrap();
    let mut child = Command::new(EXE)
        .args(["mcp", "--agent", "test-agent"])
        .env("CRASTU_SOCKET", tmp.path().join("crastu.sock"))
        .env("CRASTU_DATA", tmp.path().join("data"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let _cleanup = Cleanup {
        child,
        socket: tmp.path().join("crastu.sock"),
        data: tmp.path().join("data"),
    };
    let mut ask = |id: u64, method: &str, params: Value| -> Value {
        writeln!(
            stdin,
            "{}",
            json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
        )
        .unwrap();
        stdin.flush().unwrap();
        loop {
            let mut line = String::new();
            assert!(
                stdout.read_line(&mut line).unwrap() > 0,
                "the shim closed stdout"
            );
            let v: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
            if v["id"] == json!(id) {
                return v;
            }
        }
    };
    let init = ask(
        1,
        "initialize",
        json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "test", "version": "0"}}),
    );
    assert!(init["result"]["serverInfo"].is_object(), "{init}");
    let list = ask(2, "tools/list", json!({}));
    let names: Vec<&str> = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["job_cancel", "job_status", "project_list"]);
    // `_cleanup` kills the shim and stops the host it started.
}
