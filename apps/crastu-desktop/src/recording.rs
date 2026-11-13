//! The desktop window's flight recorder: a session folder for app `crastu-desktop` (the panic
//! hook and the fatal-signal handler point at it) and the once-a-second `perf.jsonl` with the
//! frame times of the window.

use crastu_recorder::{Config, Recorder};

/// The folder name under the log root.
pub const APP: &str = "crastu-desktop";

/// Starts the recorder for this window process and installs the panic hook and the crash
/// handler, on the calling thread (the main thread). It never fails: without a folder the
/// recorder is inert and says so once on stderr.
pub fn start() -> Recorder {
    let mut config = Config::new(APP, env!("CARGO_PKG_VERSION"));
    config.modules = crate::built_in_modules();
    crastu_recorder::set_resident_reader(crastu_procmem::resident_bytes);
    let recorder = Recorder::start_with(config);
    recorder.install_panic_hook();
    if let Some(dir) = recorder.session_dir() {
        match crastu_crash::install(dir) {
            Ok(()) | Err(crastu_crash::InstallError::Unsupported) => {}
            Err(e) => recorder.warn("crash", &format!("crash handler not installed: {e}")),
        }
    }
    recorder.info("desktop", "window opening");
    recorder
}
