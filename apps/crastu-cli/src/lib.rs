//! Headless Crastu CLI, as a library so that the desktop app answers the same commands.
//!
//! ```text
//! crastu host                  run the headless host until stopped
//! crastu mcp [--agent NAME]    stdio MCP server; starts the host if it is not running
//! crastu stop                  ask a running host to exit
//! crastu models list           the model catalogue with licences, sizes, Offline and what is installed
//! crastu models download ID [--dir PATH]   download one model (progress on stderr)
//! crastu agents add|list|revoke|rotate [NAME]   pair MCP agents with a token
//! crastu version
//! ```
//!
//! `crastu` is the desktop binary (`crastu-desktop`), which hands these commands to [`run`] before
//! it opens a window; `crastu-cli` is the same commands without the window, for machines and CI
//! that have no GPU. `CRASTU_SOCKET` and `CRASTU_DATA` override the socket path and the data
//! folder. `CRASTU_OFFLINE=1` (or the `network.offline` setting) refuses every network request,
//! including model downloads. Add to Claude Code with: `claude mcp add --transport stdio crastu -- crastu mcp`.
//!
//! An agent name that a client declares is only a label. `crastu agents add NAME` prints a token
//! once; the shim reads it from the environment variable `CRASTU_AGENT_TOKEN` and sends it with
//! every call, and only then does the host apply that agent's own policy. Without a token the
//! caller is unpaired and gets the defaults.

mod models;
mod recording;

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
    let recorder = recording::start(program, &args);
    let result = match args.first().map(String::as_str) {
        Some("host") => run_host(recorder.clone()),
        Some("mcp") => run_mcp(flag(&args, "--agent").unwrap_or_else(|| "mcp-client".into())),
        Some("stop") => stop(),
        Some("models") => models::run(
            &args,
            || data_dir().map(|d| d.join("models")),
            &offline_gate,
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
    recording::finish(recorder.as_ref(), &result);
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

/// Where the database and the settings live: the lookup the desktop app uses too.
fn data_dir() -> Result<PathBuf, String> {
    crastu_settings::data_dir()
}

/// The gate for this run: `CRASTU_OFFLINE` when it holds a recognised value, otherwise the
/// `network.offline` setting in `settings.json` in the data folder (the file the desktop app
/// uses). A settings file that cannot be read is an error rather than a silent "online".
fn offline_gate() -> Result<crastu_core::NetGate, String> {
    let env = offline_env(std::env::var_os(crastu_settings::OFFLINE_ENV));
    gate_from(env.as_deref(), crastu_settings::settings_path)
}

/// The text of `CRASTU_OFFLINE`. A value that is not valid Unicode is not "unset": it becomes an
/// unrecognised word, which `parse_offline_env` reads as on (fail closed).
fn offline_env(value: Option<std::ffi::OsString>) -> Option<String> {
    value.map(|v| {
        v.into_string()
            .unwrap_or_else(|_| "not valid unicode".to_string())
    })
}

fn gate_from(
    env: Option<&str>,
    settings_path: impl FnOnce() -> Result<PathBuf, String>,
) -> Result<crastu_core::NetGate, String> {
    if let Some(on) = crastu_settings::parse_offline_env(env) {
        return Ok(crastu_core::NetGate::new(on));
    }
    // Without a data folder the setting cannot be found, and guessing "online" could let a
    // request out that the user turned off: ask for CRASTU_DATA or an explicit CRASTU_OFFLINE.
    let path = settings_path().map_err(|e| {
        format!(
            "cannot tell whether Offline is on, the settings file cannot be located ({e}); \
             set CRASTU_DATA, or set {}=1 or 0 to choose",
            crastu_settings::OFFLINE_ENV
        )
    })?;
    let settings = crastu_settings::Settings::load(&path).map_err(|e| {
        format!(
            "cannot read {} ({e}); fix it, or set {}=1 or 0 to choose",
            path.display(),
            crastu_settings::OFFLINE_ENV
        )
    })?;
    Ok(crastu_core::NetGate::new(crastu_settings::offline_enabled(
        &settings, None,
    )))
}

fn run_host(recorder: Option<crastu_recorder::Recorder>) -> Result<(), String> {
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
            let recorder = recorder.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(50));
                let _ = std::fs::remove_file(path);
                // `exit` skips destructors, so the clean end is written here.
                if let Some(r) = &recorder {
                    r.info("host", "stopping on request");
                    r.finish(crastu_recorder::Outcome::Ok);
                }
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

    fn settings_file(json: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, json).unwrap();
        (dir, path)
    }

    #[test]
    fn the_gate_follows_the_settings_file_and_the_environment() {
        let (_d, on) = settings_file(r#"{"network.offline": true}"#);
        let p = on.clone();
        assert!(gate_from(None, || Ok(p)).unwrap().is_offline());
        let p = on.clone();
        assert!(!gate_from(Some("0"), || Ok(p)).unwrap().is_offline());
        let (_d, off) = settings_file("{}");
        let p = off.clone();
        assert!(!gate_from(None, || Ok(p)).unwrap().is_offline());
        assert!(gate_from(Some("yes"), || Ok(off)).unwrap().is_offline());
    }

    #[test]
    fn a_missing_file_means_online_and_a_broken_file_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("settings.json");
        assert!(!gate_from(None, || Ok(missing)).unwrap().is_offline());
        let (_d, bad) = settings_file("{ not json");
        let e = gate_from(None, || Ok(bad.clone())).unwrap_err();
        assert!(e.contains("CRASTU_OFFLINE"), "{e}");
        // The environment still decides without reading the broken file.
        assert!(gate_from(Some("1"), || Ok(bad)).unwrap().is_offline());
    }

    #[test]
    fn without_a_data_folder_the_gate_asks_instead_of_guessing_online() {
        let e = gate_from(None, || Err("no home".into())).unwrap_err();
        assert!(
            e.contains("CRASTU_DATA") && e.contains("CRASTU_OFFLINE"),
            "{e}"
        );
        // An empty or blank variable defers to the setting, so it fails the same way.
        assert!(gate_from(Some(" "), || Err("no home".into())).is_err());
        // The environment variable alone is enough, and a typo fails closed.
        assert!(
            !gate_from(Some("0"), || Err("no home".into()))
                .unwrap()
                .is_offline()
        );
        assert!(
            gate_from(Some("1"), || Err("no home".into()))
                .unwrap()
                .is_offline()
        );
        assert!(
            gate_from(Some("ture"), || Err("no home".into()))
                .unwrap()
                .is_offline()
        );
    }

    #[test]
    fn the_offline_variable_is_read_without_losing_odd_values() {
        use std::ffi::OsString;
        assert_eq!(offline_env(None), None);
        assert_eq!(offline_env(Some(OsString::from("1"))).as_deref(), Some("1"));
        assert_eq!(offline_env(Some(OsString::new())).as_deref(), Some(""));
    }

    #[cfg(unix)]
    #[test]
    fn a_non_unicode_offline_variable_means_on_even_if_the_setting_is_off() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;
        let env = offline_env(Some(OsString::from_vec(vec![0x66, 0xff, 0xfe])));
        assert!(env.is_some(), "not read as unset");
        let (_d, off) = settings_file("{}");
        assert!(gate_from(env.as_deref(), || Ok(off)).unwrap().is_offline());
    }
}
