//! The flight recorder for the commands that do real work (`host`, `mcp`, `models`): a session
//! folder under the log root (`CRASTU_LOG_DIR` moves it), a panic hook, and `end.json` on exit.
//! `stop`, `agents`, `version` and bad usage are too short to be worth a folder.

use crastu_recorder::{Config, Recorder};

/// Commands that are recorded.
const RECORDED: [&str; 3] = ["host", "mcp", "models"];

pub(crate) fn start(program: &str, args: &[String]) -> Option<Recorder> {
    let command = args.first()?.as_str();
    if !RECORDED.contains(&command) {
        return None;
    }
    let recorder = Recorder::start_with(Config::new(program, env!("CARGO_PKG_VERSION")));
    recorder.install_panic_hook();
    // The command name only: arguments can hold paths and agent names.
    recorder.info("cli", &format!("command {command}"));
    Some(recorder)
}

/// Records how the command ended and writes `end.json`. An error is a normal exit of the
/// process (it is reported and the exit code says so), so the folder gets `end.json` either way;
/// the error text goes to the event log, redacted.
pub(crate) fn finish(recorder: Option<&Recorder>, result: &Result<(), String>) {
    let Some(r) = recorder else { return };
    if let Err(e) = result {
        r.error("cli", &format!("command failed: {e}"));
    }
    r.finish();
}
