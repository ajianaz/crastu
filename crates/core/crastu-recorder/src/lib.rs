//! Flight recorder: one local folder per run that stays useful after a crash.
//!
//! ```text
//! <root>/<app>/<UTC start>-<pid>/
//!   session.json   at start: app, version, OS, enabled modules
//!   end.json       on a clean finish; a folder without it ended badly
//!   events.jsonl   one JSON object per line, written as it happens
//!   perf.jsonl     one `PerfSample` per line
//!   panic.txt      message, location, thread, backtrace, last events (panic hook)
//! ```
//!
//! The root is `CRASTU_LOG_DIR`, else `~/Library/Logs/Crastu` (macOS), `%LOCALAPPDATA%\Crastu\Logs`
//! (Windows) or `$XDG_STATE_HOME/crastu/logs`. If the folder cannot be created or written the
//! recorder degrades to doing nothing; it never fails the program and never panics.
//!
//! # Privacy
//!
//! What is guaranteed: job and MCP events take only a name (letters, digits and `._:-/`) and an
//! enum, so they cannot carry content; perf samples are numbers and job kinds; `session.json` and
//! `end.json` hold fixed fields. What is not: the free-text `log`, `info`, `warn` and `error`
//! messages and the panic message are written as given, cut to a length limit and with known
//! secret shapes (`sk-...`, `Bearer ...`, `crastu_<hex>` agent tokens) replaced by `[redacted]`.
//! That is a safety net for keys, not a content filter: callers must not put transcripts, audio,
//! file contents or user text into messages. Backtraces hold source paths, never data.
//!
//! Still to come, in later slices of issue #25: an async-signal-safe handler for SIGSEGV and
//! SIGBUS (`crash.txt`, needs an opt-in crate that allows `unsafe`), and Settings, About with
//! "Open logs" and "Copy diagnostics".

mod events;
mod panic;
mod paths;
mod perf;
mod redact;
mod retention;
mod time;

use std::collections::VecDeque;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use serde_json::{Map, Value, json};

pub use events::{JobEvent, JobState, Level, McpDecision, McpEvent};
pub use paths::{LOG_DIR_ENV, resolve_log_root};
pub use perf::{JobTiming, PerfSample, frame_stats};
pub use redact::redact;
pub use retention::{Retention, prune};

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

/// What to record and where.
#[derive(Clone, Debug)]
pub struct Config {
    /// The app folder name under the root: `crastu`, `crastu-cli`, `crastu-host`.
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

#[derive(Debug)]
struct State {
    events: Option<File>,
    perf: Option<File>,
    ring: VecDeque<String>,
    seq: u64,
}

#[derive(Debug)]
pub(crate) struct Inner {
    dir: Option<PathBuf>,
    started: Instant,
    state: Mutex<State>,
    finished: AtomicBool,
    pub(crate) panicked: AtomicBool,
    pub(crate) hook_installed: AtomicBool,
}

/// A session folder being written. Cheap to clone: every clone writes to the same folder, and the
/// run counts as finished once any clone calls [`finish`](Recorder::finish) or the last clone is
/// dropped without a panic in flight.
#[derive(Clone, Debug)]
pub struct Recorder {
    inner: Arc<Inner>,
}

pub(crate) fn open_append(path: &Path) -> std::io::Result<File> {
    let mut o = OpenOptions::new();
    o.create(true).append(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600);
    o.open(path)
}

fn create_private_dir(path: &Path) -> std::io::Result<()> {
    let mut b = fs::DirBuilder::new();
    b.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut b, 0o700);
    b.create(path)
}

/// Creates `<app_dir>/<stamp>-<pid>`, with a `-2`, `-3`... suffix if it exists already.
fn create_session_dir(app_dir: &Path, stamp: &str) -> Option<PathBuf> {
    create_private_dir(app_dir).ok()?;
    let base = format!("{stamp}-{}", std::process::id());
    for n in 1..100 {
        let name = if n == 1 {
            base.clone()
        } else {
            format!("{base}-{n}")
        };
        let path = app_dir.join(name);
        let mut b = fs::DirBuilder::new();
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut b, 0o700);
        match b.create(&path) {
            Ok(()) => return Some(path),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return None,
        }
    }
    None
}

impl Recorder {
    /// Starts a session for `app` and `version`, with the default root and retention.
    /// `root_override` moves the root. Never fails: when the folder cannot be created the
    /// recorder is inert (see [`session_dir`](Recorder::session_dir)).
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
        let root = match &config.root_override {
            Some(r) => Some(r.clone()),
            None => resolve_log_root(platform, env).ok(),
        };
        let now = Utc::now();
        let dir = root.and_then(|root| {
            let app_dir = root.join(redact::ident(&config.app));
            let dir = create_session_dir(&app_dir, &now.stamp())?;
            retention::prune(&app_dir, &dir, config.retention);
            Some(dir)
        });
        let state = State {
            events: dir
                .as_ref()
                .and_then(|d| open_append(&d.join(EVENTS_FILE)).ok()),
            perf: dir
                .as_ref()
                .and_then(|d| open_append(&d.join(PERF_FILE)).ok()),
            ring: VecDeque::new(),
            seq: 0,
        };
        let rec = Recorder {
            inner: Arc::new(Inner {
                dir,
                started: Instant::now(),
                state: Mutex::new(state),
                finished: AtomicBool::new(false),
                panicked: AtomicBool::new(false),
                hook_installed: AtomicBool::new(false),
            }),
        };
        rec.write_json(
            SESSION_FILE,
            &json!({
                "app": config.app,
                "version": config.version,
                "os": std::env::consts::OS,
                "arch": std::env::consts::ARCH,
                "pid": std::process::id(),
                "started": now.rfc3339(),
                "modules": config.modules,
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
            let _ = fs::write(dir.join(file), text);
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
        let Ok(mut line) = serde_json::to_string(&Value::Object(obj)) else {
            return;
        };
        if st.ring.len() >= RING_CAP {
            st.ring.pop_front();
        }
        st.ring.push_back(line.clone());
        line.push('\n');
        // One write per line, straight to the file (no buffer), so a crash loses nothing.
        if let Some(f) = st.events.as_mut()
            && f.write_all(line.as_bytes()).is_err()
        {
            st.events = None;
        }
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
        let Ok(mut line) = serde_json::to_string(&Value::Object(obj)) else {
            return;
        };
        line.push('\n');
        let mut st = self.state();
        if let Some(f) = st.perf.as_mut()
            && f.write_all(line.as_bytes()).is_err()
        {
            st.perf = None;
        }
    }

    /// Writes `panic.txt` when any thread panics (message, location, thread, backtrace, the last
    /// events), then calls the hook that was installed before. Installs once per recorder.
    pub fn install_panic_hook(&self) {
        panic::install(&self.inner);
    }

    /// Marks a clean end by writing `end.json`. Only the first call writes; a run that panicked
    /// never writes it.
    pub fn finish(&self) {
        self.inner.finish();
    }
}

impl Inner {
    fn finish(&self) {
        if self.panicked.load(Ordering::SeqCst) || self.finished.swap(true, Ordering::SeqCst) {
            return;
        }
        if let Some(dir) = &self.dir {
            let value = json!({
                "ended": Utc::now().rfc3339(),
                "clean": true,
                "duration_ms": self.started.elapsed().as_millis() as u64,
            });
            if let Ok(text) = serde_json::to_string_pretty(&value) {
                let _ = fs::write(dir.join(END_FILE), text);
            }
        }
    }
}

impl Drop for Inner {
    /// Dropping the last handle during normal flow counts as a clean end; unwinding does not.
    fn drop(&mut self) {
        if !std::thread::panicking() {
            self.finish();
        }
    }
}

#[cfg(test)]
mod tests;
