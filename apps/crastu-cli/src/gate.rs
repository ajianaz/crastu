//! The Offline gate for CLI commands: `crastu models` builds one per run, `crastu host` keeps one
//! for its lifetime and hands it to the [`Host`], so refused requests from tool calls reach the
//! activity log.
//!
//! Both start from [`source`]: `CRASTU_OFFLINE` when it holds a recognised value, otherwise the
//! `network.offline` setting in `settings.json` in the data folder (the file the desktop app
//! uses). `models` stops with an error when the setting cannot be found or read; the host keeps
//! running with Offline ON and says so on stderr (fail closed).

use std::path::PathBuf;
use std::sync::Mutex;

use crastu_core::NetGate;
use crastu_host::Host;
use crastu_ipc::method;
use serde_json::Value;

/// Where the Offline switch comes from for this process.
#[derive(Debug)]
enum Source {
    /// `CRASTU_OFFLINE` decides, and the setting is never read.
    Env(bool),
    /// The setting in this file decides.
    File(PathBuf),
    /// The settings file cannot be located; the message says why.
    Unknown(String),
}

/// The text of `CRASTU_OFFLINE`. A value that is not valid Unicode is not "unset": it becomes an
/// unrecognised word, which `parse_offline_env` reads as on (fail closed).
pub(crate) fn offline_env(value: Option<std::ffi::OsString>) -> Option<String> {
    value.map(|v| {
        v.into_string()
            .unwrap_or_else(|_| "not valid unicode".to_string())
    })
}

fn env_now() -> Option<String> {
    offline_env(std::env::var_os(crastu_settings::OFFLINE_ENV))
}

fn source(env: Option<&str>, settings_path: impl FnOnce() -> Result<PathBuf, String>) -> Source {
    if let Some(on) = crastu_settings::parse_offline_env(env) {
        return Source::Env(on);
    }
    match settings_path() {
        Ok(path) => Source::File(path),
        Err(e) => Source::Unknown(e),
    }
}

/// Whether Offline is on, or why that cannot be told.
fn resolve(source: &Source) -> Result<bool, String> {
    let path = match source {
        Source::Env(on) => return Ok(*on),
        Source::File(path) => path,
        // Without a data folder the setting cannot be found, and guessing "online" could let a
        // request out that the user turned off: ask for CRASTU_DATA or an explicit CRASTU_OFFLINE.
        Source::Unknown(e) => {
            return Err(format!(
                "cannot tell whether Offline is on, the settings file cannot be located ({e}); \
                 set CRASTU_DATA, or set {}=1 or 0 to choose",
                crastu_settings::OFFLINE_ENV
            ));
        }
    };
    let settings = crastu_settings::Settings::load(path).map_err(|e| {
        format!(
            "cannot read {} ({e}); fix it, or set {}=1 or 0 to choose",
            path.display(),
            crastu_settings::OFFLINE_ENV
        )
    })?;
    Ok(crastu_settings::offline_enabled(&settings, None))
}

/// The gate for one `models` run. A settings file that cannot be found or read is an error
/// rather than a silent "online".
pub(crate) fn offline_gate() -> Result<NetGate, String> {
    gate_from(env_now().as_deref(), crastu_settings::settings_path)
}

fn gate_from(
    env: Option<&str>,
    settings_path: impl FnOnce() -> Result<PathBuf, String>,
) -> Result<NetGate, String> {
    resolve(&source(env, settings_path)).map(NetGate::new)
}

/// The gate a running host shares with its tools. The desktop app can flip `network.offline`
/// while the host runs, so the setting is read again before every tool call (the file is small;
/// reading it each time has no timestamp-granularity gaps). `CRASTU_OFFLINE` is read once and,
/// when set, decides for the whole run.
#[derive(Debug)]
pub(crate) struct HostGate {
    gate: NetGate,
    source: Source,
    /// The last problem reported on stderr, so a broken file is reported once, not per call.
    reported: Mutex<Option<String>>,
}

impl HostGate {
    /// The host's gate for this process's environment and data folder.
    pub(crate) fn from_env() -> HostGate {
        HostGate::new(env_now().as_deref(), crastu_settings::settings_path)
    }

    fn new(env: Option<&str>, settings_path: impl FnOnce() -> Result<PathBuf, String>) -> HostGate {
        let hg = HostGate {
            gate: NetGate::new(true),
            source: source(env, settings_path),
            reported: Mutex::new(None),
        };
        hg.refresh();
        hg
    }

    pub(crate) fn gate(&self) -> &NetGate {
        &self.gate
    }

    /// Reads the setting again (a no-op when the environment decides). A setting that cannot be
    /// found or read turns Offline on; the first time a given problem shows up, its one-line
    /// description is printed on stderr and returned.
    pub(crate) fn refresh(&self) -> Option<String> {
        if let Source::Env(on) = self.source {
            self.gate.set_offline(on);
            return None;
        }
        let mut reported = self.reported.lock().unwrap_or_else(|p| p.into_inner());
        match resolve(&self.source) {
            Ok(on) => {
                self.gate.set_offline(on);
                *reported = None;
                None
            }
            Err(e) => {
                self.gate.set_offline(true);
                if reported.as_deref() == Some(e.as_str()) {
                    return None;
                }
                let line = format!("Offline is on: {e}");
                eprintln!("crastu host: {line}");
                *reported = Some(e);
                Some(line)
            }
        }
    }

    /// Answers one IPC request for `host`, reading the setting again before a tool call.
    pub(crate) fn handle(&self, host: &Host, m: &str, p: Value) -> Result<Value, String> {
        if m == method::TOOLS_CALL {
            self.refresh();
        }
        host.handle_ipc(m, p)
    }
}

#[cfg(test)]
#[path = "gate_tests.rs"]
mod tests;
