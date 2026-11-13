//! Model downloads for Settings, Models and licences: `crastu-models` over a [`Fetch`] adapter,
//! behind the shared Offline gate.
//!
//! A download runs on its own thread and only ever pushes [`DownloadEvent`]s into a queue; the
//! UI drains the queue on its own thread (`ModelsPlatform::download_events`), so the background
//! work never touches the UI. Progress events of one model are coalesced: the queue keeps the
//! newest. Every id comes from the catalogue's rows and is looked up in the catalogue; the file
//! names that reach the disk are the catalogue's, which `Catalogue::new` has checked to be plain
//! file names (no path, no `..`), and removal touches nothing but those names and their `.part`
//! inside the models folder.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use crastu_app::NetGate;
use crastu_models::{
    Cancel, Catalogue, Fetch, GatedFetch, ModelEntry, ModelError, Progress, download_with,
    is_present,
};
use crastu_shell::models::DownloadEvent;

/// Builds the adapter that opens the requests. Called on the download's thread, because a
/// [`Fetch`] need not be `Send`.
pub type MakeFetch = Arc<dyn Fn() -> Box<dyn Fetch> + Send + Sync>;

/// `Box<dyn Fetch>` as a [`Fetch`] (the crate implements the trait for references only).
struct Boxed(Box<dyn Fetch>);

impl Fetch for Boxed {
    fn fetch(&self, url: &str, offset: u64) -> io::Result<crastu_models::FetchResponse> {
        self.0.fetch(url, offset)
    }
}

#[derive(Default)]
struct Shared {
    /// The downloads in flight, with the flag that stops each.
    active: Mutex<HashMap<String, Arc<Cancel>>>,
    events: Mutex<Vec<DownloadEvent>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl Shared {
    fn push(&self, event: DownloadEvent) {
        let mut events = lock(&self.events);
        if let DownloadEvent::Progress { id, .. } = &event
            && let Some(slot) = events
                .iter_mut()
                .find(|e| matches!(e, DownloadEvent::Progress { id: other, .. } if other == id))
        {
            *slot = event;
            return;
        }
        events.push(event);
    }
}

pub struct ModelStore {
    dir: PathBuf,
    catalogue: Catalogue,
    gate: NetGate,
    fetch: MakeFetch,
    shared: Arc<Shared>,
}

impl ModelStore {
    pub fn new(dir: PathBuf, catalogue: Catalogue, gate: NetGate, fetch: MakeFetch) -> ModelStore {
        ModelStore {
            dir,
            catalogue,
            gate,
            fetch,
            shared: Arc::default(),
        }
    }

    fn entry(&self, id: &str) -> Result<&ModelEntry, String> {
        self.catalogue
            .get(id)
            .ok_or_else(|| "that model is not in the catalogue".to_string())
    }

    pub fn installed(&self) -> Vec<String> {
        self.catalogue
            .entries()
            .iter()
            .filter(|e| is_present(e, &self.dir))
            .map(|e| e.id.clone())
            .collect()
    }

    /// Starts the download on a new thread. The gate is asked again when the request opens, so a
    /// switch to Offline between the click and the connection still refuses it.
    pub fn start(&self, id: &str) -> Result<(), String> {
        let entry = self.entry(id)?.clone();
        let cancel = Arc::new(Cancel::new());
        {
            let mut active = lock(&self.shared.active);
            if active.contains_key(id) {
                return Err("it is already downloading".to_string());
            }
            active.insert(id.to_string(), cancel.clone());
        }
        let (shared, dir, gate, make) = (
            self.shared.clone(),
            self.dir.clone(),
            self.gate.clone(),
            self.fetch.clone(),
        );
        let spawned = std::thread::Builder::new()
            .name(format!("model-download-{id}"))
            .spawn(move || {
                let outcome = catch_unwind(AssertUnwindSafe(|| {
                    let fetch = GatedFetch::new(Boxed(make()), gate, &entry.id);
                    let mut progress = |p: Progress| {
                        shared.push(DownloadEvent::Progress {
                            id: entry.id.clone(),
                            done: p.done,
                            total: p.total,
                        })
                    };
                    download_with(&entry, &dir, &fetch, &cancel, &mut progress)
                }));
                let id = entry.id;
                let event = match outcome {
                    Ok(Ok(_)) => DownloadEvent::Done { id: id.clone() },
                    Ok(Err(ModelError::Cancelled)) => DownloadEvent::Cancelled { id: id.clone() },
                    Ok(Err(e)) => DownloadEvent::Failed {
                        id: id.clone(),
                        message: e.to_string(),
                    },
                    Err(_) => DownloadEvent::Failed {
                        id: id.clone(),
                        message: "the download stopped unexpectedly".to_string(),
                    },
                };
                // Free the slot first: once the UI sees the end, a new download may start.
                lock(&shared.active).remove(&id);
                shared.push(event);
            });
        if let Err(e) = spawned {
            lock(&self.shared.active).remove(id);
            return Err(format!("could not start the download: {e}"));
        }
        Ok(())
    }

    pub fn cancel(&self, id: &str) {
        if let Some(cancel) = lock(&self.shared.active).get(id) {
            cancel.cancel();
        }
    }

    /// Deletes the model file, its partial download and the empty lock file of that download,
    /// all inside the models folder.
    pub fn remove(&self, id: &str) -> Result<(), String> {
        let entry = self.entry(id)?;
        // Held until the end: a start() of this model waits, so it cannot begin writing a part
        // file that this call then deletes.
        let active = lock(&self.shared.active);
        if active.contains_key(id) {
            return Err("it is downloading; cancel the download first".to_string());
        }
        let name = plain_name(&entry.file_name)?;
        // The empty `.part.lock` goes too, so a removed model leaves nothing behind. Only a
        // download takes that lock, and none of this model runs in this process now.
        for file in [
            name.to_string(),
            format!("{name}.part"),
            format!("{name}.part.lock"),
        ] {
            match fs::remove_file(self.dir.join(&file)) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(format!("{file}: {e}")),
            }
        }
        drop(active);
        Ok(())
    }

    pub fn events(&self) -> Vec<DownloadEvent> {
        std::mem::take(&mut *lock(&self.shared.events))
    }
}

/// The name itself when it is one plain file name, an error otherwise. The catalogue already
/// guarantees it; this keeps the deletion safe if that ever changes.
fn plain_name(name: &str) -> Result<&str, String> {
    let mut parts = Path::new(name).components();
    match (parts.next(), parts.next()) {
        (Some(Component::Normal(n)), None) if n == name => Ok(name),
        _ => Err("the model's file name is not a plain file name".to_string()),
    }
}

#[cfg(test)]
mod tests;
