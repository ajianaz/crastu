//! Headless Crastu CLI, as a library so that the desktop app answers the same commands.
//!
//! ```text
//! crastu host                  run the headless host until stopped
//! crastu mcp [--agent NAME]    stdio MCP server; starts the host if it is not running
//! crastu stop                  ask a running host to exit
//! crastu models list           the model catalogue with licences and sizes
//! crastu models download ID [--dir PATH]   download one model (progress on stderr)
//! crastu agents add|list|revoke|rotate [NAME]   pair MCP agents with a token
//! crastu version
//! ```
//!
//! `crastu` is the desktop binary (`crastu-desktop`), which hands these commands to [`run`] before
//! it opens a window; `crastu-cli` is the same commands without the window, for machines and CI
//! that have no GPU. `CRASTU_SOCKET` and `CRASTU_DATA` override the socket path and the data
//! folder. Add to Claude Code with: `claude mcp add --transport stdio crastu -- crastu mcp`.
//!
//! An agent name that a client declares is only a label. `crastu agents add NAME` prints a token
//! once; the shim reads it from the environment variable `CRASTU_AGENT_TOKEN` and sends it with
//! every call, and only then does the host apply that agent's own policy. Without a token the
//! caller is unpaired and gets the defaults.

mod models;

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crastu_host::{DenyAsk, Host};
use crastu_ipc::{Client, Handler, method};
use crastu_jobs::JobQueue;
use crastu_store::Store;
use serde_json::{Value, json};

mod agents;

const SHUTDOWN: &str = "host.shutdown";

/// The commands this library answers; anything else is not for it.
pub const COMMANDS: [&str; 7] = [
    "host",
    "mcp",
    "stop",
    "models",
    "agents",
    "version",
    "--version",
];

/// Runs one command. `program` is the name shown in messages (`crastu` or `crastu-cli`) and
/// `args` are the arguments after it.
pub fn run(program: &str, args: Vec<String>) -> ExitCode {
    let result = match args.first().map(String::as_str) {
        Some("host") => run_host(),
        Some("mcp") => run_mcp(flag(&args, "--agent").unwrap_or_else(|| "mcp-client".into())),
        Some("stop") => stop(),
        Some("models") => models::run(
            &args,
            || data_dir().map(|d| d.join("models")),
            &crastu_models_http::HttpFetch::new(),
            &mut std::io::stdout(),
            &mut std::io::stderr(),
        ),
        Some("agents") => data_dir().and_then(|d| agents::run(program, &d, &args)),
        Some("version") | Some("--version") => {
            println!("{program} {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        _ => {
            eprintln!(
                "usage: {program} <host | mcp [--agent NAME] | stop | {} | agents | version>",
                models::USAGE
            );
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{program}: {e}");
            ExitCode::FAILURE
        }
    }
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

/// Where the database lives. No shared temp-directory fallback: without `HOME` (and without
/// `XDG_DATA_HOME` off macOS) the host asks for `CRASTU_DATA` instead.
fn data_dir() -> Result<PathBuf, String> {
    if let Some(p) = std::env::var_os("CRASTU_DATA") {
        return Ok(PathBuf::from(p));
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let dir = if cfg!(target_os = "macos") {
        home.map(|h| h.join("Library/Application Support/Crastu"))
    } else if let Some(x) = std::env::var_os("XDG_DATA_HOME") {
        Some(PathBuf::from(x).join("crastu"))
    } else {
        home.map(|h| h.join(".local/share/crastu"))
    };
    dir.ok_or_else(|| {
        "HOME is not set, so there is no per-user data directory; set CRASTU_DATA".into()
    })
}

fn run_host() -> Result<(), String> {
    let dir = data_dir()?;
    // The database holds projects and the activity log: files the host creates are private to
    // the user (umask 077), in a directory that belongs to the user.
    crastu_ipc::private_file_mode();
    crastu_ipc::ensure_owned_dir(&dir).map_err(|e| format!("cannot use {}: {e}", dir.display()))?;
    let store = Store::open(dir.join("crastu.db")).map_err(|e| e.to_string())?;
    let cpus = std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(2);
    // No window: nobody can answer Ask, so Ask is refused with a pointer to the app.
    let host = Arc::new(
        Host::new(store, JobQueue::new(cpus, 1), vec![], Box::new(DenyAsk))
            .map_err(|e| e.to_string())?,
    );

    let socket = crastu_ipc::default_socket_path().map_err(|e| e.to_string())?;
    let listener = crastu_ipc::bind(&socket).map_err(|e| e.to_string())?;
    let socket_for_exit = socket.clone();
    let handler: Handler = Arc::new(move |m, p| {
        if m == SHUTDOWN {
            let path = socket_for_exit.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(50));
                let _ = std::fs::remove_file(path);
                std::process::exit(0);
            });
            return Ok(json!({"stopping": true}));
        }
        host.handle_ipc(m, p)
    });
    eprintln!("crastu host listening on {}", socket.display());
    crastu_ipc::serve(listener, handler);
    Ok(())
}

/// A connection to a host that answers, if one is running.
fn live_host(socket: &Path) -> Option<Client> {
    let mut c = Client::connect(socket).ok()?;
    c.call(method::PING, Value::Null).ok()?;
    Some(c)
}

/// The command that starts the host in the background. The host does not need the agent's
/// token, so it must not inherit it from the shim that starts it.
fn host_command(exe: &Path) -> Command {
    let mut c = Command::new(exe);
    c.arg("host")
        .env_remove(agents::TOKEN_ENV)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    c
}

/// Connects to the host, starting the host (`<this program> host`) in the background when none answers.
fn connect_or_start(socket: &Path) -> Result<Client, String> {
    if let Some(c) = live_host(socket) {
        return Ok(c);
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    host_command(&exe)
        .spawn()
        .map_err(|e| format!("cannot start the host: {e}"))?;
    let start = Instant::now();
    loop {
        if let Some(c) = live_host(socket) {
            return Ok(c);
        }
        if start.elapsed() > Duration::from_secs(10) {
            return Err(format!("the host did not start at {}", socket.display()));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn run_mcp(agent: String) -> Result<(), String> {
    let socket = crastu_ipc::default_socket_path().map_err(|e| e.to_string())?;
    let client = connect_or_start(&socket)?;
    let backend = Arc::new(crastu_mcp::IpcBackend::new(client));
    let mut bridge = crastu_mcp::McpBridge::new(backend, agent);
    // From the environment, never from an argument. A token that is set but wrong is refused by
    // the host on every call; it is not dropped here, so it cannot turn into an unpaired caller.
    if let Some(token) = std::env::var_os(agents::TOKEN_ENV) {
        let token = token
            .into_string()
            .map_err(|_| format!("{} is not valid text", agents::TOKEN_ENV))?;
        bridge = bridge.with_token(token);
    }
    let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    rt.block_on(crastu_mcp::serve_stdio(bridge))
}

fn stop() -> Result<(), String> {
    let socket = crastu_ipc::default_socket_path().map_err(|e| e.to_string())?;
    let mut c = Client::connect(&socket).map_err(|_| "no host is running".to_string())?;
    c.call(SHUTDOWN, Value::Null).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_host_does_not_inherit_the_agent_token() {
        let cmd = host_command(Path::new("crastu"));
        let envs: Vec<_> = cmd.get_envs().collect();
        assert!(
            envs.iter()
                .any(|(k, v)| *k == agents::TOKEN_ENV && v.is_none()),
            "CRASTU_AGENT_TOKEN must be removed from the host's environment: {envs:?}"
        );
        assert_eq!(cmd.get_args().collect::<Vec<_>>(), ["host"]);
    }
}
