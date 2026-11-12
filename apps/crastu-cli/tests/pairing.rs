//! The pairing flow through the real binary: `agents add` prints a token once, the shim sends it
//! from `CRASTU_AGENT_TOKEN`, and the host marks the call as that agent's. No token is an
//! unpaired caller; a wrong or revoked token is an error and never a fallback.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};

const EXE: &str = env!("CARGO_BIN_EXE_crastu-cli");

struct Env {
    _tmp: tempfile::TempDir,
    socket: PathBuf,
    data: PathBuf,
}

impl Env {
    fn new() -> Env {
        let tmp = tempfile::tempdir().unwrap();
        Env {
            socket: tmp.path().join("crastu.sock"),
            data: tmp.path().join("data"),
            _tmp: tmp,
        }
    }

    fn cmd(&self) -> Command {
        let mut c = Command::new(EXE);
        c.env("CRASTU_SOCKET", &self.socket)
            .env("CRASTU_DATA", &self.data)
            .env_remove("CRASTU_AGENT_TOKEN");
        c
    }

    fn agents(&self, args: &[&str]) -> (bool, String) {
        let out = self.cmd().arg("agents").args(args).output().unwrap();
        let text = String::from_utf8_lossy(&out.stdout).into_owned()
            + &String::from_utf8_lossy(&out.stderr);
        (out.status.success(), text)
    }

    /// Runs `crastu-cli mcp` with `token` in the environment, calls `project_list` once and
    /// returns the tool result.
    fn call_project_list(&self, declared: &str, token: Option<&str>) -> Value {
        let mut cmd = self.cmd();
        cmd.args(["mcp", "--agent", declared]);
        if let Some(t) = token {
            cmd.env("CRASTU_AGENT_TOKEN", t);
        }
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        request(
            &mut stdin,
            &mut stdout,
            1,
            "initialize",
            json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}}),
        );
        writeln!(
            stdin,
            "{}",
            json!({"jsonrpc": "2.0", "method": "notifications/initialized"})
        )
        .unwrap();
        let reply = request(
            &mut stdin,
            &mut stdout,
            2,
            "tools/call",
            json!({"name": "project_list", "arguments": {}}),
        );
        drop(stdin);
        let _ = child.wait();
        reply["result"].clone()
    }

    fn activity(&self) -> Vec<(String, bool, String)> {
        let conn = crastu_store::rusqlite::Connection::open(self.data.join("crastu.db")).unwrap();
        let mut stmt = conn
            .prepare("SELECT agent, paired, decision FROM activity ORDER BY id")
            .unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? != 0, r.get(2)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    fn stop_host(&self) {
        let status = self.cmd().arg("stop").status().unwrap();
        assert!(status.success());
    }
}

fn request(
    stdin: &mut ChildStdin,
    stdout: &mut BufReader<ChildStdout>,
    id: u64,
    method: &str,
    params: Value,
) -> Value {
    let msg = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
    writeln!(stdin, "{msg}").unwrap();
    stdin.flush().unwrap();
    loop {
        let mut line = String::new();
        assert!(stdout.read_line(&mut line).unwrap() > 0, "server closed");
        let v: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
        if v["id"] == json!(id) {
            return v;
        }
    }
}

fn token_in(output: &str) -> String {
    output
        .split_whitespace()
        .find(|w| w.starts_with("crastu_") && w.len() > 20)
        .expect("a token in the output")
        .to_string()
}

fn text(result: &Value) -> &str {
    result["content"][0]["text"].as_str().unwrap_or_default()
}

#[test]
fn pair_call_rotate_and_revoke() {
    let env = Env::new();

    let (ok, out) = env.agents(&["add", "claude-code"]);
    assert!(ok, "{out}");
    let token = token_in(&out);
    assert!(
        out.contains(&format!("--env CRASTU_AGENT_TOKEN={token}")),
        "{out}"
    );

    let (_, list) = env.agents(&["list"]);
    assert!(list.contains("claude-code") && list.contains("active"));
    assert!(!list.contains(&token), "list must never show a token");
    let (ok, dup) = env.agents(&["add", "claude-code"]);
    assert!(!ok && dup.contains("already exists"), "{dup}");

    // Paired: the declared name is ignored, the pairing names the agent.
    let paired = env.call_project_list("some-other-name", Some(&token));
    assert_ne!(paired["isError"], json!(true), "{paired}");
    // Unpaired: works with the defaults, marked unpaired under the declared name.
    let unpaired = env.call_project_list("claude-code", None);
    assert_ne!(unpaired["isError"], json!(true), "{unpaired}");
    // A wrong token is an error, not a fallback to unpaired.
    let wrong = env.call_project_list("claude-code", Some("crastu_nope"));
    assert_eq!(wrong["isError"], json!(true), "{wrong}");
    assert!(text(&wrong).contains("not valid"), "{wrong}");

    // Rotate: the old token stops, the new one works.
    let (ok, out) = env.agents(&["rotate", "claude-code"]);
    assert!(ok, "{out}");
    let new_token = token_in(&out);
    assert_ne!(new_token, token);
    let old = env.call_project_list("x", Some(&token));
    assert_eq!(old["isError"], json!(true), "{old}");
    let fresh = env.call_project_list("x", Some(&new_token));
    assert_ne!(fresh["isError"], json!(true), "{fresh}");

    // Revoke: the token stops, the record stays.
    let (ok, out) = env.agents(&["revoke", "claude-code"]);
    assert!(ok, "{out}");
    let revoked = env.call_project_list("x", Some(&new_token));
    assert_eq!(revoked["isError"], json!(true), "{revoked}");
    assert!(text(&revoked).contains("revoked"), "{revoked}");
    let (_, list) = env.agents(&["list"]);
    assert!(list.contains("revoked"), "{list}");

    env.stop_host();
    let log = env.activity();
    let row = |a: &str, p: bool, d: &str| (a.to_string(), p, d.to_string());
    assert_eq!(
        log,
        vec![
            row("claude-code", true, "auto"),
            row("claude-code", false, "auto"),
            row("claude-code", false, "bad token"),
            row("x", false, "bad token"),
            row("claude-code", true, "auto"),
            row("x", false, "bad token"),
        ]
    );
}

#[test]
fn the_database_file_never_holds_the_token() {
    let env = Env::new();
    let (_, out) = env.agents(&["add", "tole"]);
    let token = token_in(&out);
    let mut bytes = Vec::new();
    for entry in std::fs::read_dir(Path::new(&env.data)).unwrap() {
        bytes.extend(std::fs::read(entry.unwrap().path()).unwrap());
    }
    assert!(!bytes.windows(token.len()).any(|w| w == token.as_bytes()));
}

#[test]
fn agents_needs_a_valid_subcommand() {
    let env = Env::new();
    for args in [&["add"][..], &["bogus"], &["add", "Bad Name"]] {
        let (ok, out) = env.agents(args);
        assert!(!ok, "{args:?}: {out}");
    }
}
