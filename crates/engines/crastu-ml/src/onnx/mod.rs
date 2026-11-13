//! The ONNX Runtime backed [`SessionFactory`] (feature `ort`).
//!
//! Facts this module relies on, checked 2026-11-13 against the `ort` 2.0.0-rc.15 source in the
//! cargo registry (lib.rs, environment.rs, session/builder) and docs/spikes/onnx-runtime.md:
//!
//! - `load-dynamic` links no native code at build time; the library is loaded by
//!   `ort::init_from(path)`. Without it, the first `ort` call panics when the library is missing,
//!   so the factory always resolves the path itself first and turns every failure into an error.
//! - `ort` asks for ONNX Runtime API level 18 with the `api-18` feature used here, and accepts any
//!   runtime of that minor version or newer (an older one is `BadVersion`; a newer one only logs
//!   an info line). The shared runtime of decision 28, sherpa-onnx's 1.28.2, is newer, so it loads.
//!   crates.io lists rc.15 as built for ONNX Runtime 1.31 (its default `api-31`); with only
//!   `api-18` it does not need 1.31. The two sources differ in what "supported" means: the `ort`
//!   docs speak of the version it was designed for, the code checks only the minimum.
//! - The default environment has telemetry ON (`EnvironmentBuilder::telemetry` starts as true and
//!   Microsoft's builds honour it), which Crastu's no-telemetry rule forbids: it is switched off.
//! - Only one environment per process; the library handle is a process-wide `OnceLock`, so the
//!   first path that loads wins and a later different path is ignored. [`environment`] keeps the
//!   first environment for the life of the process.

mod options;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use ort::environment::Environment;
use ort::session::Session;

pub use ort;

use crate::lib_path;
use crate::{ModelId, SessionError, SessionFactory, SessionOptions};

/// One ONNX session. `ort` runs a session through `&mut`, so the session sits behind a lock;
/// several callers of one shared session take turns (runs on one session are serialised).
#[derive(Debug)]
pub struct OrtSession {
    session: Mutex<Session>,
    model_bytes: u64,
}

impl OrtSession {
    /// The session, locked for one run. A panic in an earlier run does not poison it for good.
    pub fn lock(&self) -> MutexGuard<'_, Session> {
        self.session.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Size of the model file the session was made from.
    pub fn model_bytes(&self) -> u64 {
        self.model_bytes
    }
}

static ENVIRONMENT: Mutex<Option<(PathBuf, Environment)>> = Mutex::new(None);

/// A runtime is already loaded from `loaded`; a request for a different file is an error, not a
/// silent use of the first one (decision 28: one runtime per process). `None` means "search",
/// which accepts whatever is loaded.
fn check_same_library(loaded: &Path, requested: Option<&Path>) -> Result<(), String> {
    let Some(requested) = requested else {
        return Ok(());
    };
    let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    if canon(loaded) == canon(requested) {
        Ok(())
    } else {
        Err(format!(
            "ONNX Runtime is already loaded from `{}`; one runtime per process (decision 28), \
             so `{}` cannot be used as well",
            loaded.display(),
            requested.display()
        ))
    }
}

/// Loads the runtime (once per process) and returns the shared environment. `library` is the
/// file to load, or `None` to search ([`lib_path::find_in_process`]).
fn environment(library: Option<&Path>) -> Result<Environment, String> {
    let mut slot = ENVIRONMENT.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some((loaded, env)) = slot.as_ref() {
        check_same_library(loaded, library)?;
        return Ok(env.clone());
    }
    let path = match library {
        Some(path) => {
            let path = lib_path::absolute(path);
            lib_path::check_not_writable_by_others(&path).map_err(|e| e.to_string())?;
            path
        }
        None => lib_path::find_in_process().map_err(|e| e.to_string())?,
    };
    let env = ort::init_from(&path)
        .map_err(|e| format!("cannot load the ONNX Runtime library: {e}"))?
        .with_name("crastu")
        // Off on purpose (no telemetry). ONNX Runtime has a telemetry provider on Windows only
        // (ETW), so this call changes something there and is a no-op elsewhere; `ort` has no
        // getter, so no test can read it back.
        .with_telemetry(false)
        .build()
        .map_err(|e| {
            format!(
                "cannot start ONNX Runtime from `{}`: {e} (only one ONNX Runtime environment \
                 is allowed per process; another user of `ort` in this process may hold it)",
                path.display()
            )
        })?;
    *slot = Some((path, env.clone()));
    Ok(env)
}

/// Builds ONNX sessions for the registry. Models are registered by id first: the registry key is
/// an identity (a changed file is a new id), not a path. Clones share the registered models, so
/// one clone can go into the registry and another stay with the code that registers.
#[derive(Clone, Debug, Default)]
pub struct OrtFactory {
    library: Option<PathBuf>,
    models: Arc<Mutex<HashMap<ModelId, PathBuf>>>,
}

impl OrtFactory {
    /// A factory that searches for the runtime: next to the executable, then `ORT_DYLIB_PATH`.
    pub fn new() -> Self {
        Self::default()
    }

    /// A factory that loads the runtime from exactly this file.
    pub fn with_library(path: impl Into<PathBuf>) -> Self {
        Self {
            library: Some(path.into()),
            ..Self::default()
        }
    }

    /// Tells the factory where the file of `model` is. A second call replaces the path.
    pub fn register_model(&self, model: ModelId, path: impl Into<PathBuf>) {
        self.models
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(model, path.into());
    }

    /// Loads the runtime now, so a missing library shows at start-up and not at the first session.
    pub fn preload(&self) -> Result<(), SessionError> {
        environment(self.library.as_deref())
            .map(drop)
            .map_err(|reason| SessionError::new(&ModelId::new("(onnx runtime)"), reason))
    }
}

impl SessionFactory for OrtFactory {
    type Session = OrtSession;

    fn create(
        &self,
        model: &ModelId,
        options: &SessionOptions,
    ) -> Result<OrtSession, SessionError> {
        let fail = |reason: String| SessionError::new(model, reason);
        let path = self
            .models
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(model)
            .cloned()
            .ok_or_else(|| fail("no file is registered for this model".to_string()))?;
        let model_bytes = std::fs::metadata(&path)
            .map_err(|e| fail(format!("cannot read `{}`: {e}", path.display())))?
            .len();
        let env = environment(self.library.as_deref()).map_err(fail)?;
        let builder = Session::builder(&env).map_err(|e| fail(e.to_string()))?;
        let mut builder = options::apply(builder, options).map_err(fail)?;
        let session = builder
            .commit_from_file(&path)
            .map_err(|e| fail(format!("cannot load `{}`: {e}", path.display())))?;
        Ok(OrtSession {
            session: Mutex::new(session),
            model_bytes,
        })
    }

    fn approx_memory(&self, session: &OrtSession) -> Option<u64> {
        // The size of the model file: a lower bound. It ignores external weight files, which an
        // ONNX model may reference, and the runtime's working memory.
        Some(session.model_bytes())
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod same_library_tests {
    use super::*;

    #[test]
    fn a_different_library_is_refused_and_search_accepts_what_is_loaded() {
        let a = Path::new("/opt/a/libonnxruntime.dylib");
        let b = Path::new("/opt/b/libonnxruntime.dylib");
        assert!(check_same_library(a, None).is_ok());
        assert!(check_same_library(a, Some(a)).is_ok());
        let err = check_same_library(a, Some(b)).unwrap_err();
        assert!(err.contains("/opt/a/") && err.contains("/opt/b/"), "{err}");
        assert!(err.contains("one runtime per process"), "{err}");
    }
}
