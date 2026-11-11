//! Talks to `crastu-cli mcp` the way an MCP client such as Claude Code does: JSON-RPC lines over
//! the child's stdin and stdout. The shim must start the host on its own, list the tools, run a
//! read tool, and refuse a change tool because a headless host cannot ask for approval.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};

struct Mcp {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl Mcp {
    fn send(&mut self, msg: Value) {
        writeln!(self.stdin, "{msg}").unwrap();
        self.stdin.flush().unwrap();
    }

    fn reply(&mut self, id: u64) -> Value {
        loop {
            let mut line = String::new();
            assert!(
                self.stdout.read_line(&mut line).unwrap() > 0,
                "server closed stdout"
            );
            let v: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
            if v["id"] == json!(id) {
                return v;
            }
        }
    }

    fn request(&mut self, id: u64, method: &str, params: Value) -> Value {
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        self.reply(id)
    }
}

#[test]
fn claude_style_session_over_stdio() {
    let tmp = tempfile::tempdir().unwrap();
    let socket = tmp.path().join("crastu.sock");
    let data = tmp.path().join("data");
    let exe = env!("CARGO_BIN_EXE_crastu-cli");

    let mut child = Command::new(exe)
        .args(["mcp", "--agent", "test-agent"])
        .env("CRASTU_SOCKET", &socket)
        .env("CRASTU_DATA", &data)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let mut mcp = Mcp {
        stdin: child.stdin.take().unwrap(),
        stdout: BufReader::new(child.stdout.take().unwrap()),
        child,
    };

    let init = mcp.request(
        1,
        "initialize",
        json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "test", "version": "0"}}),
    );
    assert!(init["result"]["serverInfo"].is_object(), "{init}");
    mcp.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));

    let list = mcp.request(2, "tools/list", json!({}));
    let names: Vec<&str> = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["job_cancel", "job_status", "project_list"]);

    let read = mcp.request(
        3,
        "tools/call",
        json!({"name": "project_list", "arguments": {}}),
    );
    assert_ne!(read["result"]["isError"], json!(true), "{read}");
    assert_eq!(read["result"]["content"][0]["text"], "[]");

    let change = mcp.request(
        4,
        "tools/call",
        json!({"name": "job_cancel", "arguments": {"job": "0193a7e2-0000-7000-8000-000000000000"}}),
    );
    assert_eq!(change["result"]["isError"], json!(true), "{change}");
    assert!(
        change["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("needs approval")
    );

    drop(mcp.stdin);
    let _ = mcp.child.wait();

    // The host the shim started is still running; stop it and check the activity reached the log.
    let status = Command::new(exe)
        .arg("stop")
        .env("CRASTU_SOCKET", &socket)
        .status()
        .unwrap();
    assert!(status.success());
    let db = rusqlite_count(&data.join("crastu.db"));
    assert_eq!(db, 2, "one read and one blocked change were logged");
}

fn rusqlite_count(path: &std::path::Path) -> i64 {
    let conn = crastu_store::rusqlite::Connection::open(path).unwrap();
    conn.query_row(
        "SELECT count(*) FROM activity WHERE agent = 'test-agent'",
        [],
        |r| r.get(0),
    )
    .unwrap()
}
