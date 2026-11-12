//! The session registry: one session per (model, options) per process, reference counted.

use std::collections::HashMap;
use std::ops::Deref;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use crate::clock::{Clock, SystemClock};
use crate::options::{ModelId, SessionOptions};

/// A session could not be created.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("cannot create a session for {model}: {reason}")]
pub struct SessionError {
    pub model: String,
    pub reason: String,
}

impl SessionError {
    pub fn new(model: &ModelId, reason: impl Into<String>) -> Self {
        Self {
            model: model.to_string(),
            reason: reason.into(),
        }
    }
}

/// Builds sessions. The real implementation wraps the ONNX runtime; tests use a fake.
pub trait SessionFactory: Send + Sync + 'static {
    type Session: Send + Sync + 'static;

    /// Creates a session. Called without any registry lock held, so it may be slow.
    fn create(
        &self,
        model: &ModelId,
        options: &SessionOptions,
    ) -> Result<Self::Session, SessionError>;

    /// Approximate bytes the session holds, if the factory can tell.
    fn approx_memory(&self, _session: &Self::Session) -> Option<u64> {
        None
    }
}

type Key = (ModelId, SessionOptions);

struct Entry<S> {
    session: Arc<S>,
    handles: usize,
    /// When the last handle dropped; `None` while a handle is alive.
    idle_since: Option<Duration>,
    memory: Option<u64>,
}

enum Slot<S> {
    /// One thread is creating the session; others wait on the condvar.
    Creating,
    Ready(Entry<S>),
}

struct Inner<F: SessionFactory> {
    factory: F,
    clock: Arc<dyn Clock>,
    idle: Duration,
    state: Mutex<HashMap<Key, Slot<F::Session>>>,
    changed: Condvar,
}

type State<'a, F> = MutexGuard<'a, HashMap<Key, Slot<<F as SessionFactory>::Session>>>;

impl<F: SessionFactory> Inner<F> {
    fn lock(&self) -> State<'_, F> {
        // The state is never left half-updated across a panic (the factory runs unlocked), so a
        // poisoned lock is safe to keep using.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Removes sessions idle for at least the idle time. The sessions are returned so the caller
    /// drops them after releasing the lock (a session's destructor may be slow).
    fn take_expired(&self, state: &mut State<'_, F>) -> Vec<Arc<F::Session>> {
        let now = self.clock.now();
        let mut freed = Vec::new();
        state.retain(|_, slot| match slot {
            Slot::Ready(entry) => match entry.idle_since {
                Some(since) if now.saturating_sub(since) >= self.idle => {
                    freed.push(Arc::clone(&entry.session));
                    false
                }
                _ => true,
            },
            Slot::Creating => true,
        });
        freed
    }
}

/// Removes the `Creating` marker if the creating thread fails or panics, and wakes the waiters.
struct CreatingGuard<'a, F: SessionFactory> {
    inner: &'a Inner<F>,
    key: &'a Key,
}

impl<F: SessionFactory> Drop for CreatingGuard<'_, F> {
    fn drop(&mut self) {
        let mut state = self.inner.lock();
        if matches!(state.get(self.key), Some(Slot::Creating)) {
            state.remove(self.key);
        }
        drop(state);
        self.inner.changed.notify_all();
    }
}

/// A counted reference to a session. Dropping the last one starts the idle timer.
pub struct Handle<F: SessionFactory> {
    inner: Arc<Inner<F>>,
    key: Key,
    session: Arc<F::Session>,
}

impl<F: SessionFactory> Handle<F> {
    pub fn model(&self) -> &ModelId {
        &self.key.0
    }

    pub fn options(&self) -> &SessionOptions {
        &self.key.1
    }
}

impl<F: SessionFactory> Deref for Handle<F> {
    type Target = F::Session;

    fn deref(&self) -> &F::Session {
        &self.session
    }
}

impl<F: SessionFactory> Clone for Handle<F> {
    fn clone(&self) -> Self {
        if let Some(Slot::Ready(entry)) = self.inner.lock().get_mut(&self.key) {
            entry.handles += 1;
        }
        Self {
            inner: Arc::clone(&self.inner),
            key: self.key.clone(),
            session: Arc::clone(&self.session),
        }
    }
}

impl<F: SessionFactory> Drop for Handle<F> {
    fn drop(&mut self) {
        let now = self.inner.clock.now();
        let mut state = self.inner.lock();
        if let Some(Slot::Ready(entry)) = state.get_mut(&self.key) {
            entry.handles = entry.handles.saturating_sub(1);
            if entry.handles == 0 {
                entry.idle_since = Some(now);
            }
        }
        // With a zero idle time the session goes at once; otherwise a later call sweeps it.
        let freed = self.inner.take_expired(&mut state);
        drop(state);
        drop(freed);
    }
}

impl<F: SessionFactory> std::fmt::Debug for Handle<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Handle")
            .field("model", &self.key.0)
            .field("options", &self.key.1)
            .finish()
    }
}

/// One live session in a [`Snapshot`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionStats {
    pub model: ModelId,
    pub options: SessionOptions,
    /// Live handles; 0 means the session is idle and waiting to expire.
    pub handles: usize,
    pub approx_memory: Option<u64>,
}

/// A point-in-time view of the registry, for the flight recorder and Settings.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    /// Sorted by model, then options.
    pub sessions: Vec<SessionStats>,
}

impl Snapshot {
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    pub fn total_handles(&self) -> usize {
        self.sessions.iter().map(|s| s.handles).sum()
    }

    /// Live handles on one model, over all its option sets.
    pub fn handles_for(&self, model: &ModelId) -> usize {
        self.sessions
            .iter()
            .filter(|s| &s.model == model)
            .map(|s| s.handles)
            .sum()
    }

    /// Sum of the sizes the factory reported; `None` when it reported none.
    pub fn approx_memory(&self) -> Option<u64> {
        self.sessions
            .iter()
            .filter_map(|s| s.approx_memory)
            .fold(None, |sum, m| Some(sum.unwrap_or(0) + m))
    }
}

/// The registry. Cheap to clone; every clone shares the same sessions.
pub struct Registry<F: SessionFactory> {
    inner: Arc<Inner<F>>,
}

impl<F: SessionFactory> Clone for Registry<F> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<F: SessionFactory> std::fmt::Debug for Registry<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Registry")
            .field("idle", &self.inner.idle)
            .finish_non_exhaustive()
    }
}

impl<F: SessionFactory> Registry<F> {
    /// A registry on the real clock. `idle` is how long a session with no handle is kept.
    pub fn new(factory: F, idle: Duration) -> Self {
        Self::with_clock(factory, idle, Arc::new(SystemClock::default()))
    }

    pub fn with_clock(factory: F, idle: Duration, clock: Arc<dyn Clock>) -> Self {
        let inner = Inner {
            factory,
            clock,
            idle,
            state: Mutex::new(HashMap::new()),
            changed: Condvar::new(),
        };
        Self {
            inner: Arc::new(inner),
        }
    }

    /// Returns a handle on the session for this model and options, creating it if none exists.
    /// Concurrent loads of one key create one session; a failed create is not cached.
    pub fn load(
        &self,
        model: &ModelId,
        options: SessionOptions,
    ) -> Result<Handle<F>, SessionError> {
        let inner = &self.inner;
        let key: Key = (model.clone(), options);
        let mut state = inner.lock();
        loop {
            let freed = inner.take_expired(&mut state);
            let existing = match state.get_mut(&key) {
                Some(Slot::Ready(entry)) => {
                    entry.handles += 1;
                    entry.idle_since = None;
                    Some(Some(Arc::clone(&entry.session)))
                }
                Some(Slot::Creating) => Some(None),
                None => None,
            };
            match existing {
                Some(Some(session)) => {
                    drop(state);
                    drop(freed);
                    return Ok(Handle {
                        inner: Arc::clone(inner),
                        key,
                        session,
                    });
                }
                Some(None) => {
                    if freed.is_empty() {
                        state = inner
                            .changed
                            .wait(state)
                            .unwrap_or_else(PoisonError::into_inner);
                    } else {
                        // Free outside the lock (a destructor may be slow or panic), then look
                        // again: the create may have finished while the lock was free.
                        drop(state);
                        drop(freed);
                        state = inner.lock();
                    }
                }
                None => {
                    state.insert(key.clone(), Slot::Creating);
                    drop(state);
                    drop(freed);
                    break;
                }
            }
        }

        // Only this thread creates this key; the lock is not held while the factory runs.
        let guard = CreatingGuard { inner, key: &key };
        let session = inner.factory.create(model, &options)?;
        let memory = inner.factory.approx_memory(&session);
        let session = Arc::new(session);
        let entry = Entry {
            session: Arc::clone(&session),
            handles: 1,
            idle_since: None,
            memory,
        };
        inner.lock().insert(key.clone(), Slot::Ready(entry));
        drop(guard); // sees Ready, removes nothing, wakes the waiters
        Ok(Handle {
            inner: Arc::clone(inner),
            key,
            session,
        })
    }

    /// Frees sessions idle past the idle time and returns how many went. Expiry also runs on
    /// every load, drop and snapshot, so calling this on a timer only frees memory sooner.
    pub fn sweep(&self) -> usize {
        let mut state = self.inner.lock();
        let freed = self.inner.take_expired(&mut state);
        drop(state);
        freed.len()
    }

    pub fn snapshot(&self) -> Snapshot {
        let mut state = self.inner.lock();
        let freed = self.inner.take_expired(&mut state);
        let mut sessions: Vec<SessionStats> = state
            .iter()
            .filter_map(|((model, options), slot)| match slot {
                Slot::Ready(e) => Some(SessionStats {
                    model: model.clone(),
                    options: *options,
                    handles: e.handles,
                    approx_memory: e.memory,
                }),
                Slot::Creating => None,
            })
            .collect();
        drop(state);
        drop(freed);
        sessions.sort_by(|a, b| (&a.model, a.options).cmp(&(&b.model, b.options)));
        Snapshot { sessions }
    }
}
