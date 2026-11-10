//! Job queue with GPU/CPU gate, progress, cancel and events.
//!
//! Jobs run on worker threads. Each [`Gate`] has its own number of slots, so heavy GPU work
//! (transcription, rendering) never runs more often than the machine can handle, while light CPU
//! jobs keep flowing. Every state change is published as a [`JobEvent`]; persistence and UI
//! updates subscribe to those events.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

use crastu_core::JobId;
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Gate {
    Cpu,
    Gpu,
}

#[derive(Clone, Debug, PartialEq)]
pub enum JobStatus {
    Queued,
    Running { progress: f32, message: String },
    Done(Value),
    Failed(String),
    Cancelled,
}

impl JobStatus {
    pub fn is_finished(&self) -> bool {
        matches!(
            self,
            JobStatus::Done(_) | JobStatus::Failed(_) | JobStatus::Cancelled
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct JobEvent {
    pub id: JobId,
    pub kind: String,
    pub status: JobStatus,
}

/// Handed to a running job.
#[derive(Debug)]
pub struct JobCtx {
    id: JobId,
    kind: String,
    cancelled: Arc<AtomicBool>,
    shared: Arc<Shared>,
}

impl JobCtx {
    pub fn id(&self) -> JobId {
        self.id
    }

    /// Jobs should check this between steps and return early when it is true.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    /// Reports progress in 0.0..=1.0 with a short human message.
    pub fn progress(&self, progress: f32, message: impl Into<String>) {
        self.shared.update(
            self.id,
            &self.kind,
            JobStatus::Running {
                progress: progress.clamp(0.0, 1.0),
                message: message.into(),
            },
        );
    }
}

type Work = Box<dyn FnOnce(&JobCtx) -> Result<Value, String> + Send>;

struct Pending {
    id: JobId,
    kind: String,
    work: Work,
}

struct Entry {
    kind: String,
    status: JobStatus,
    cancelled: Arc<AtomicBool>,
}

#[derive(Default)]
struct State {
    jobs: HashMap<JobId, Entry>,
    queues: HashMap<Gate, VecDeque<Pending>>,
    subscribers: Vec<Sender<JobEvent>>,
    shutdown: bool,
}

#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    wake: Condvar,
}

impl std::fmt::Debug for Shared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Shared")
    }
}

impl Shared {
    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn update(&self, id: JobId, kind: &str, status: JobStatus) {
        let mut st = self.lock();
        if let Some(e) = st.jobs.get_mut(&id) {
            if e.status.is_finished() {
                return;
            }
            e.status = status.clone();
        }
        let ev = JobEvent {
            id,
            kind: kind.to_string(),
            status,
        };
        st.subscribers.retain(|s| s.send(ev.clone()).is_ok());
    }
}

/// The queue. Dropping it stops the workers after their current job.
pub struct JobQueue {
    shared: Arc<Shared>,
    workers: Vec<JoinHandle<()>>,
}

impl std::fmt::Debug for JobQueue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JobQueue")
            .field("workers", &self.workers.len())
            .finish()
    }
}

impl JobQueue {
    pub fn new(cpu_slots: usize, gpu_slots: usize) -> JobQueue {
        let shared = Arc::new(Shared::default());
        let mut workers = Vec::new();
        for (gate, n) in [(Gate::Cpu, cpu_slots.max(1)), (Gate::Gpu, gpu_slots.max(1))] {
            for _ in 0..n {
                let shared = shared.clone();
                workers.push(std::thread::spawn(move || worker(shared, gate)));
            }
        }
        JobQueue { shared, workers }
    }

    pub fn subscribe(&self) -> Receiver<JobEvent> {
        let (tx, rx) = channel();
        self.shared.lock().subscribers.push(tx);
        rx
    }

    pub fn submit<F>(&self, kind: &str, gate: Gate, work: F) -> JobId
    where
        F: FnOnce(&JobCtx) -> Result<Value, String> + Send + 'static,
    {
        let id = JobId::new();
        {
            let mut st = self.shared.lock();
            st.jobs.insert(
                id,
                Entry {
                    kind: kind.to_string(),
                    status: JobStatus::Queued,
                    cancelled: Arc::new(AtomicBool::new(false)),
                },
            );
            st.queues.entry(gate).or_default().push_back(Pending {
                id,
                kind: kind.to_string(),
                work: Box::new(work),
            });
        }
        self.shared.update(id, kind, JobStatus::Queued);
        self.shared.wake.notify_all();
        id
    }

    pub fn status(&self, id: JobId) -> Option<JobStatus> {
        self.shared.lock().jobs.get(&id).map(|e| e.status.clone())
    }

    /// Cancels a queued job immediately, or asks a running job to stop.
    /// Returns false when the job is unknown or already finished.
    pub fn cancel(&self, id: JobId) -> bool {
        let kind = {
            let mut st = self.shared.lock();
            let Some(e) = st.jobs.get(&id) else {
                return false;
            };
            if e.status.is_finished() {
                return false;
            }
            e.cancelled.store(true, Ordering::Relaxed);
            let kind = e.kind.clone();
            let was_queued = e.status == JobStatus::Queued;
            if was_queued {
                for q in st.queues.values_mut() {
                    q.retain(|p| p.id != id);
                }
            }
            if !was_queued {
                return true;
            }
            kind
        };
        self.shared.update(id, &kind, JobStatus::Cancelled);
        true
    }

    /// Jobs that have not finished, oldest first.
    pub fn active(&self) -> Vec<(JobId, String, JobStatus)> {
        let st = self.shared.lock();
        let mut v: Vec<_> = st
            .jobs
            .iter()
            .filter(|(_, e)| !e.status.is_finished())
            .map(|(id, e)| (*id, e.kind.clone(), e.status.clone()))
            .collect();
        v.sort_by_key(|(id, _, _)| *id);
        v
    }
}

impl Drop for JobQueue {
    fn drop(&mut self) {
        self.shared.lock().shutdown = true;
        self.shared.wake.notify_all();
        for w in self.workers.drain(..) {
            let _ = w.join();
        }
    }
}

fn worker(shared: Arc<Shared>, gate: Gate) {
    loop {
        let (pending, cancelled) = {
            let mut st = shared.lock();
            loop {
                if st.shutdown {
                    return;
                }
                if let Some(p) = st.queues.get_mut(&gate).and_then(VecDeque::pop_front) {
                    let flag = st
                        .jobs
                        .get(&p.id)
                        .map(|e| e.cancelled.clone())
                        .unwrap_or_default();
                    break (p, flag);
                }
                st = shared.wake.wait(st).unwrap_or_else(|p| p.into_inner());
            }
        };
        let Pending { id, kind, work } = pending;
        shared.update(
            id,
            &kind,
            JobStatus::Running {
                progress: 0.0,
                message: String::new(),
            },
        );
        let ctx = JobCtx {
            id,
            kind: kind.clone(),
            cancelled: cancelled.clone(),
            shared: shared.clone(),
        };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(&ctx)));
        let status = if cancelled.load(Ordering::Relaxed) {
            JobStatus::Cancelled
        } else {
            match result {
                Ok(Ok(v)) => JobStatus::Done(v),
                Ok(Err(e)) => JobStatus::Failed(e),
                Err(_) => JobStatus::Failed("job panicked".into()),
            }
        };
        shared.update(id, &kind, status);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::AtomicUsize;
    use std::time::{Duration, Instant};

    fn wait_for(q: &JobQueue, id: JobId) -> JobStatus {
        let start = Instant::now();
        loop {
            let s = q.status(id).unwrap();
            if s.is_finished() {
                return s;
            }
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "job did not finish"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn runs_jobs_and_reports_events() {
        let q = JobQueue::new(2, 1);
        let events = q.subscribe();
        let id = q.submit("transcribe", Gate::Gpu, |ctx| {
            ctx.progress(0.5, "half way");
            Ok(json!({"words": 3}))
        });
        assert_eq!(wait_for(&q, id), JobStatus::Done(json!({"words": 3})));
        let seen: Vec<JobStatus> = events
            .try_iter()
            .filter(|e| e.id == id)
            .map(|e| e.status)
            .collect();
        assert_eq!(seen.first(), Some(&JobStatus::Queued));
        assert!(seen.contains(&JobStatus::Running {
            progress: 0.5,
            message: "half way".into()
        }));
        assert_eq!(seen.last(), Some(&JobStatus::Done(json!({"words": 3}))));
    }

    #[test]
    fn failures_and_panics_become_failed() {
        let q = JobQueue::new(1, 1);
        let a = q.submit("x", Gate::Cpu, |_| Err("model not downloaded".into()));
        let b = q.submit("x", Gate::Cpu, |_| panic!("boom"));
        assert_eq!(
            wait_for(&q, a),
            JobStatus::Failed("model not downloaded".into())
        );
        assert_eq!(wait_for(&q, b), JobStatus::Failed("job panicked".into()));
    }

    #[test]
    fn gpu_gate_runs_one_at_a_time() {
        let q = JobQueue::new(4, 1);
        let running = Arc::new(AtomicUsize::new(0));
        let max_seen = Arc::new(AtomicUsize::new(0));
        let ids: Vec<JobId> = (0..4)
            .map(|_| {
                let (running, max_seen) = (running.clone(), max_seen.clone());
                q.submit("render", Gate::Gpu, move |_| {
                    let now = running.fetch_add(1, Ordering::SeqCst) + 1;
                    max_seen.fetch_max(now, Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(10));
                    running.fetch_sub(1, Ordering::SeqCst);
                    Ok(Value::Null)
                })
            })
            .collect();
        for id in ids {
            wait_for(&q, id);
        }
        assert_eq!(max_seen.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn cancel_queued_and_running() {
        let q = JobQueue::new(1, 1);
        let (started_tx, started_rx) = channel();
        let running = q.submit("long", Gate::Cpu, move |ctx| {
            started_tx.send(()).unwrap();
            while !ctx.is_cancelled() {
                std::thread::sleep(Duration::from_millis(1));
            }
            Ok(Value::Null)
        });
        let queued = q.submit("later", Gate::Cpu, |_| Ok(Value::Null));
        started_rx.recv().unwrap();
        assert!(q.cancel(queued));
        assert_eq!(q.status(queued), Some(JobStatus::Cancelled));
        assert!(q.cancel(running));
        assert_eq!(wait_for(&q, running), JobStatus::Cancelled);
        assert!(!q.cancel(running), "already finished");
        assert!(q.active().is_empty());
    }
}
