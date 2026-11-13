//! Flight recorder: one local folder per run that stays useful after a crash.
//!
//! ```text
//! <root>/<app>/<UTC start>-<pid>/
//!   session.json   at start: app, version, OS, enabled modules
//!   session.lock   held by the running process; tells a later run this folder is alive
//!   end.json       when the run ends; a folder without it ended badly
//!   events.jsonl   one JSON object per line, written as it happens
//!   perf.jsonl     one `PerfSample` per line
//!   panic.txt      message, location, thread, backtrace, last events (panic hook)
//!   crash.txt      fatal signal report, written by `crastu-crash` (not this crate)
//! ```
//!
//! The root is `CRASTU_LOG_DIR`, else `~/Library/Logs/Crastu` (macOS), `%LOCALAPPDATA%\Crastu\Logs`
//! (Windows) or `$XDG_STATE_HOME/crastu/logs`. If the folder cannot be created the recorder
//! prints one line on stderr (`flight recorder disabled: ...`) and does nothing else; it never
//! fails the program and never panics. A write that fails is counted (`dropped_lines` in
//! `end.json`) and the stream tries again on the next write. A live session cannot fill the
//! disk: `events.jsonl` stops at 20 MB and `perf.jsonl` at 10 MB (one marker line, then later
//! lines are dropped and counted), and `panic.txt` keeps the first five panics in full, then one
//! line per later panic, up to 256 KB.
//!
//! `end.json` holds `status` (`ok`, `error` or `panicked`), `clean` (true only for `ok`),
//! `panics` (panics seen by the hook, caught ones included), `dropped_lines`, `duration_ms` and
//! `ended`. A run that panics out of `main` never writes it; a panic after `finish` rewrites it
//! as `panicked`.
//!
//! # Privacy
//!
//! What is guaranteed: job and MCP events take only a plain name (letters, digits and `_:-`, so
//! never a path) and an enum, so they cannot carry content; perf samples are numbers and job
//! kinds; `session.json` and `end.json` hold fixed fields; every file is created with mode 0600
//! in a 0700 folder. What is not: the free-text `log`, `info`, `warn` and `error` messages and
//! the panic message are written as given, cut to a length limit and with known secret shapes
//! (`sk-`, `hf_`, `ghp_`, `github_pat_`, `xox*-` and AWS key ids, JWTs, `Bearer` and `Basic`
//! values, `crastu_<hex>` agent tokens, URL `user:pass@`, and the values of `token=`, `key=`,
//! `secret=`, `password=`, `signature=` and similar pairs) replaced by `[redacted]`. That is a
//! safety net for keys, not a content filter: callers must not put transcripts, audio, file
//! contents or user text into messages. Backtraces hold source paths, never data.
//!
//! Fatal signals are not handled here: this crate stays free of `unsafe`, and the binaries pass
//! [`Recorder::session_dir`] to `crastu-crash`, which writes `crash.txt` from a signal handler.
//!
//! Settings, About reads this folder back through the `crastu-diagnostics` crate ("Copy
//! diagnostics"): the version, the OS, the last events and the headers of the previous run's
//! `end.json`, `panic.txt` and `crash.txt`, redacted again and bounded.
//!
//! `perf.jsonl` is fed by the callers: the host samples the job queue and memory once a second,
//! the desktop app records frame times with [`FrameStats`]. [`resident_bytes`] reads memory from
//! `/proc` on Linux and Android; on macOS and Windows it needs `unsafe`, so an app registers the
//! `crastu-procmem` reader with [`set_resident_reader`], and without one it gives nothing.

mod events;
mod frames;
mod panic;
mod paths;
mod perf;
mod rss;
mod time;

use std::collections::VecDeque;
use std::ffi::OsString;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use serde_json::{Map, Value, json};

pub use crastu_logfiles::{Retention, prune};
pub use crastu_redact::redact;
pub use events::{JobEvent, JobState, Level, McpDecision, McpEvent};
pub use frames::{FrameStats, Frames, IDLE_MS};
pub use paths::{LOG_DIR_ENV, resolve_log_root};
pub use perf::{JobTiming, PerfSample, frame_stats};
pub use rss::{parse_vm_rss, resident_bytes, set_resident_reader};

use crastu_logfiles::{Session, Stream, create_session, write_private};
use time::Utc;

pub(crate) const PANIC_FILE: &str = "panic.txt";
const SESSION_FILE: &str = "session.json";
const END_FILE: &str = "end.json";
const EVENTS_FILE: &str = "events.jsonl";
const PERF_FILE: &str = "perf.jsonl";
/// Events kept in memory for `panic.txt`.
const RING_CAP: usize = 256;
/// Events written into `panic.txt`.
pub(crate) const RING_DUMP: usize = 100;
/// Size caps for the streams of one session; see `crastu_logfiles::Stream`.
const EVENTS_CAP: u64 = 20 * 1024 * 1024;
const PERF_CAP: u64 = 10 * 1024 * 1024;

/// What to record and where.
#[derive(Clone, Debug)]
pub struct Config {
    /// The app folder name under the root: `crastu`, `crastu-cli`, `crastu-host`. It must be a
    /// plain name (letters, digits and `_:-`); anything else disables the recorder.
    pub app: String,
    pub version: String,
    /// Moves the root; wins over `CRASTU_LOG_DIR`.
    pub root_override: Option<PathBuf>,
    /// The enabled modules, for `session.json`.
    pub modules: Vec<String>,
    pub retention: Retention,
}

impl Config {
    pub fn new(app: &str, version: &str) -> Config {
        Config {
            app: app.to_string(),
            version: version.to_string(),
            root_override: None,
            modules: Vec::new(),
            retention: Retention::default(),
        }
    }
}

/// How a run ended, for `end.json`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The run did what it was asked to.
    Ok,
    /// The run ended normally but with a failure (for example a command that returned an error).
    Error,
}

#[derive(Debug)]
pub(crate) struct State {
    events: Stream,
    perf: Stream,
    pub(crate) ring: VecDeque<String>,
    seq: u64,
}

impl State {
    /// Lines that could not be written, over both streams.
    pub(crate) fn dropped(&self) -> u64 {
        self.events.dropped + self.perf.dropped
    }
}

#[derive(Debug)]
pub(crate) struct Inner {
    pub(crate) dir: Option<PathBuf>,
    /// Held until the recorder is dropped or the process ends: marks the session as alive.
    _lock: Option<File>,
    started: Instant,
    pub(crate) state: Mutex<State>,
    pub(crate) finished: AtomicBool,
    pub(crate) panics: AtomicU64,
    pub(crate) hook_installed: AtomicBool,
}

/// A session folder being written. Cheap to clone: every clone writes to the same folder, and the
/// run counts as finished once any clone calls [`finish`](Recorder::finish) or the last clone is
/// dropped without a panic in flight.
#[derive(Clone, Debug)]
pub struct Recorder {
    inner: Arc<Inner>,
}

impl Recorder {
    /// Starts a session for `app` and `version`, with the default root and retention.
    /// `root_override` moves the root. Never fails: when the folder cannot be created the
    /// recorder says so once on stderr and is inert (see [`session_dir`](Recorder::session_dir)).
    pub fn start(app: &str, version: &str, root_override: Option<&Path>) -> Recorder {
        let mut config = Config::new(app, version);
        config.root_override = root_override.map(Path::to_path_buf);
        Recorder::start_with(config)
    }

    /// [`start`](Recorder::start) with every option, reading the real environment.
    pub fn start_with(config: Config) -> Recorder {
        Recorder::start_in(config, crastu_settings::Platform::current(), &|n| {
            std::env::var_os(n)
        })
    }

    /// [`start_with`](Recorder::start_with) with the platform and the environment injected.
    pub fn start_in(
        config: Config,
        platform: crastu_settings::Platform,
        env: &dyn Fn(&str) -> Option<OsString>,
    ) -> Recorder {
        let now = Utc::now();
        let session = open_session(&config, platform, env, &now);
        let (dir, lock) = match session {
            Ok(s) => (Some(s.dir), s.lock),
            Err(reason) => {
                eprintln!("flight recorder disabled: {reason}");
                (None, None)
            }
        };
        let path = |name: &str| dir.as_ref().map(|d| d.join(name));
        let state = State {
            events: Stream::open(path(EVENTS_FILE), EVENTS_CAP),
            perf: Stream::open(path(PERF_FILE), PERF_CAP),
            ring: VecDeque::new(),
            seq: 0,
        };
        let rec = Recorder {
            inner: Arc::new(Inner {
                dir,
                _lock: lock,
                started: Instant::now(),
                state: Mutex::new(state),
                finished: AtomicBool::new(false),
                panics: AtomicU64::new(0),
                hook_installed: AtomicBool::new(false),
            }),
        };
        rec.write_json(
            SESSION_FILE,
            &json!({
                "app": config.app,
                // Caller strings are checked like every other name: a version is cut and
                // redacted, a module must be a plain name.
                "version": crastu_redact::clean(&config.version, 64),
                "os": std::env::consts::OS,
                "arch": std::env::consts::ARCH,
                "pid": std::process::id(),
                "started": now.rfc3339(),
                "modules": config.modules.iter().map(|m| crastu_redact::ident(m)).collect::<Vec<_>>(),
            }),
        );
        rec
    }

    /// The session folder, or `None` when it could not be created.
    pub fn session_dir(&self) -> Option<&Path> {
        self.inner.dir.as_deref()
    }

    fn write_json(&self, file: &str, value: &Value) {
        if let Some(dir) = &self.inner.dir
            && let Ok(text) = serde_json::to_string_pretty(value)
        {
            let _ = write_private(&dir.join(file), text.as_bytes());
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.inner
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn emit(&self, fields: Map<String, Value>) {
        let mut st = self.state();
        st.seq += 1;
        let mut obj = Map::new();
        obj.insert("ts".into(), json!(Utc::now().rfc3339()));
        obj.insert("seq".into(), json!(st.seq));
        obj.extend(fields);
        let Ok(line) = serde_json::to_string(&Value::Object(obj)) else {
            return;
        };
        if st.ring.len() >= RING_CAP {
            st.ring.pop_front();
        }
        st.ring.push_back(line.clone());
        st.events.write_line(&line);
    }

    /// Records a log line. `msg` must not contain user content (transcripts, audio, file
    /// contents, keys): known key shapes are redacted and the text is cut to 500 characters, and
    /// that is all.
    pub fn log(&self, level: Level, target: &str, msg: &str) {
        self.emit(events::log_fields(level, target, msg));
    }

    /// [`log`](Recorder::log) at info level.
    pub fn info(&self, target: &str, msg: &str) {
        self.log(Level::Info, target, msg);
    }

    /// [`log`](Recorder::log) at warn level.
    pub fn warn(&self, target: &str, msg: &str) {
        self.log(Level::Warn, target, msg);
    }

    /// [`log`](Recorder::log) at error level.
    pub fn error(&self, target: &str, msg: &str) {
        self.log(Level::Error, target, msg);
    }

    /// Records a job state change.
    pub fn job_event(&self, event: &JobEvent<'_>) {
        self.emit(events::job_fields(event));
    }

    /// Records an MCP call: the tool name and the decision, never the content.
    pub fn mcp_event(&self, event: &McpEvent<'_>) {
        self.emit(events::mcp_fields(event));
    }

    /// Appends one sample to `perf.jsonl`.
    pub fn perf(&self, sample: &PerfSample) {
        let Ok(Value::Object(fields)) = serde_json::to_value(sample) else {
            return;
        };
        let mut obj = Map::new();
        obj.insert("ts".into(), json!(Utc::now().rfc3339()));
        obj.extend(fields);
        let Ok(line) = serde_json::to_string(&Value::Object(obj)) else {
            return;
        };
        self.state().perf.write_line(&line);
    }

    /// Writes `panic.txt` when any thread panics (message, location, thread, backtrace, the last
    /// events), then calls the hook that was installed before. Installs once per recorder.
    pub fn install_panic_hook(&self) {
        panic::install(&self.inner);
    }

    /// Ends the run by writing `end.json`. Only the first call writes. A panic after this call
    /// rewrites the file as `panicked`.
    pub fn finish(&self, outcome: Outcome) {
        self.inner.finish(outcome);
    }
}

fn open_session(
    config: &Config,
    platform: crastu_settings::Platform,
    env: &dyn Fn(&str) -> Option<OsString>,
    now: &Utc,
) -> Result<Session, String> {
    if !crastu_redact::name_ok(&config.app) {
        return Err("the app name must be a plain name (letters, digits and _:-)".into());
    }
    let root = match &config.root_override {
        Some(r) => r.clone(),
        None => resolve_log_root(platform, env)?,
    };
    let app_dir = root.join(&config.app);
    let session = create_session(&app_dir, &now.stamp())?;
    crastu_logfiles::prune(&app_dir, &session.dir, config.retention);
    Ok(session)
}

impl Inner {
    /// Writes `end.json` (replacing any earlier one).
    pub(crate) fn write_end(&self, status: &str, clean: bool, dropped: u64) {
        let Some(dir) = &self.dir else { return };
        let value = json!({
            "ended": Utc::now().rfc3339(),
            "status": status,
            "clean": clean,
            "panics": self.panics.load(Ordering::SeqCst),
            "dropped_lines": dropped,
            "duration_ms": self.started.elapsed().as_millis() as u64,
        });
        if let Ok(text) = serde_json::to_string_pretty(&value) {
            let _ = write_private(&dir.join(END_FILE), text.as_bytes());
        }
    }

    fn finish(&self, outcome: Outcome) {
        if self.finished.swap(true, Ordering::SeqCst) {
            return;
        }
        let dropped = self
            .state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .dropped();
        match outcome {
            Outcome::Ok => self.write_end("ok", true, dropped),
            Outcome::Error => self.write_end("error", false, dropped),
        }
    }
}

impl Drop for Inner {
    /// Dropping the last handle during normal flow counts as an `ok` end; unwinding does not.
    fn drop(&mut self) {
        if !std::thread::panicking() {
            self.finish(Outcome::Ok);
        }
    }
}

#[cfg(test)]
mod tests;
