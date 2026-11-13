//! The flight recorder for the commands that do real work (`host`, `mcp`, `models`): a session
//! folder under the log root (`CRASTU_LOG_DIR` moves it), a panic hook, the fatal-signal handler
//! (`crash.txt`, Unix only), and `end.json` on exit. `stop`, `agents`, `version` and bad usage
//! are too short to be worth a folder.

use crastu_recorder::{Config, Outcome, Recorder};

/// Commands that are recorded.
const RECORDED: [&str; 3] = ["host", "mcp", "models"];

pub(crate) fn start(program: &str, args: &[String]) -> Option<Recorder> {
    let command = args.first()?.as_str();
    if !RECORDED.contains(&command) {
        return None;
    }
    let recorder = Recorder::start_with(Config::new(program, env!("CARGO_PKG_VERSION")));
    recorder.install_panic_hook();
    install_crash_handler(&recorder);
    // The command name only: arguments can hold paths and agent names.
    recorder.info("cli", &format!("command {command}"));
    Some(recorder)
}

/// Points the fatal-signal handler at the session folder, on the main thread (it gets an
/// alternate signal stack if it has none). Without a folder there is nowhere to write; on a
/// system without the handler (Windows, for now) nothing is said.
fn install_crash_handler(recorder: &Recorder) {
    let Some(dir) = recorder.session_dir() else {
        return;
    };
    match crastu_crash::install(dir) {
        Ok(()) | Err(crastu_crash::InstallError::Unsupported) => {}
        Err(e) => recorder.warn("crash", &format!("crash handler not installed: {e}")),
    }
}

/// Records how the command ended and writes `end.json`: `status: ok`, or `status: error` with
/// the error text in the event log. Error texts can embed URLs and keys (a failed download names
/// its URL), so they go through the recorder's redaction like every free-text message.
pub(crate) fn finish(recorder: Option<&Recorder>, result: &Result<(), String>) {
    let Some(r) = recorder else { return };
    match result {
        Ok(()) => r.finish(Outcome::Ok),
        Err(e) => {
            r.error("cli", &format!("command failed: {e}"));
            r.finish(Outcome::Error);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(dir: &std::path::Path, file: &str) -> String {
        std::fs::read_to_string(dir.join(file)).unwrap()
    }

    #[test]
    fn a_failed_command_ends_as_an_error_and_its_text_is_redacted() {
        let root = tempfile::tempdir().unwrap();
        let r = Recorder::start("crastu-cli", "0", Some(root.path()));
        let dir = r.session_dir().unwrap().to_path_buf();
        finish(
            Some(&r),
            &Err("GET https://user:hunter2@example.com/m.bin?token=abc123: refused".into()),
        );
        let events = read(&dir, "events.jsonl");
        assert!(events.contains("command failed"), "{events}");
        assert!(
            !events.contains("hunter2") && !events.contains("abc123"),
            "{events}"
        );
        let end = read(&dir, "end.json");
        assert!(
            end.contains("\"status\": \"error\"") && end.contains("\"clean\": false"),
            "{end}"
        );
    }

    #[test]
    fn a_successful_command_ends_ok() {
        let root = tempfile::tempdir().unwrap();
        let r = Recorder::start("crastu-cli", "0", Some(root.path()));
        let dir = r.session_dir().unwrap().to_path_buf();
        finish(Some(&r), &Ok(()));
        assert!(read(&dir, "end.json").contains("\"status\": \"ok\""));
    }
}
