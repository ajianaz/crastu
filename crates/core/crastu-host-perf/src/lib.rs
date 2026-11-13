//! The host's `perf.jsonl`: once a second, the queue depth, the jobs that finished since the
//! last sample with how long they ran, and resident memory (from `/proc` on Linux
//! and Android, from the registered `crastu-procmem` reader on macOS and Windows; see
//! `crastu_recorder::resident_bytes`).
//!
//! The arithmetic is [`Sampler`], which takes its times as arguments, so tests need no clock.
//! [`start`] runs it on one thread that wakes every [`POLL`] to stamp the job events it received,
//! and writes a sample every [`INTERVAL`]. Job timings are therefore accurate to about [`POLL`].
//! The thread never panics on a bad value, ends when asked to (writing one last sample) and ends
//! by itself when its handle is dropped.

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crastu_core::JobId;
use crastu_jobs::{JobEvent, JobQueue, JobStatus};
use crastu_recorder::{JobTiming, PerfSample};

pub const INTERVAL: Duration = Duration::from_secs(1);
pub const POLL: Duration = Duration::from_millis(100);
/// Job timings in one sample; a burst of more finished jobs is cut (the rest are dropped).
const MAX_JOBS: usize = 64;

/// What one sample needs from the running host.
pub trait Source: Send + 'static {
    /// Job events received since the last call.
    fn events(&mut self) -> Vec<JobEvent>;
    /// Jobs waiting for a slot.
    fn queue_depth(&self) -> u32;
    fn rss_bytes(&self) -> Option<u64>;
}

/// Turns job events and the numbers at the moment of a sample into a [`PerfSample`].
#[derive(Debug, Default)]
pub struct Sampler {
    /// When each running job started, in milliseconds on the caller's clock.
    started: HashMap<JobId, u64>,
    finished: Vec<JobTiming>,
}

impl Sampler {
    /// A job event that arrived at `now_ms`. A job counts when it finished (done, failed or
    /// cancelled) after it was seen running; one cancelled while queued ran for no time and is
    /// left out.
    pub fn event(&mut self, ev: &JobEvent, now_ms: u64) {
        match &ev.status {
            JobStatus::Running { .. } => {
                self.started.entry(ev.id).or_insert(now_ms);
            }
            JobStatus::Queued => {}
            JobStatus::Done(_) | JobStatus::Failed(_) | JobStatus::Cancelled => {
                if let Some(start) = self.started.remove(&ev.id)
                    && self.finished.len() < MAX_JOBS
                {
                    self.finished
                        .push(JobTiming::new(&ev.kind, now_ms.saturating_sub(start)));
                }
            }
        }
    }

    /// The sample for this moment; starts a new window of job timings.
    pub fn sample(&mut self, queue_depth: u32, rss_bytes: Option<u64>) -> PerfSample {
        PerfSample {
            rss_bytes,
            queue_depth: Some(queue_depth),
            jobs: std::mem::take(&mut self.finished),
            ..PerfSample::default()
        }
    }
}

/// Reads the real queue.
struct QueueSource {
    queue: Arc<JobQueue>,
    events: Receiver<JobEvent>,
}

impl Source for QueueSource {
    fn events(&mut self) -> Vec<JobEvent> {
        self.events.try_iter().collect()
    }

    fn queue_depth(&self) -> u32 {
        let queued = self
            .queue
            .active()
            .iter()
            .filter(|(_, _, s)| *s == JobStatus::Queued)
            .count();
        u32::try_from(queued).unwrap_or(u32::MAX)
    }

    fn rss_bytes(&self) -> Option<u64> {
        crastu_recorder::resident_bytes()
    }
}

/// A running sampler thread. Stopping it writes one last sample.
#[derive(Debug)]
pub struct PerfFeed {
    stop: Sender<()>,
    thread: JoinHandle<()>,
}

impl PerfFeed {
    /// Asks the thread to end and waits for it.
    pub fn stop(self) {
        let _ = self.stop.send(());
        let _ = self.thread.join();
    }
}

/// Starts the feed for the host's job queue, writing to `recorder`.
pub fn start(recorder: &crastu_recorder::Recorder, queue: &Arc<JobQueue>) -> Option<PerfFeed> {
    let source = QueueSource {
        events: queue.subscribe(),
        queue: queue.clone(),
    };
    let recorder = recorder.clone();
    spawn(source, move |s| recorder.perf(s), INTERVAL, POLL)
}

/// The feed on any source and sink, with the timings given (tests pass tiny ones).
pub fn spawn(
    mut source: impl Source,
    sink: impl Fn(&PerfSample) + Send + 'static,
    interval: Duration,
    poll: Duration,
) -> Option<PerfFeed> {
    let (stop, stopped) = channel::<()>();
    let thread = std::thread::Builder::new()
        .name("crastu-perf".into())
        .spawn(move || {
            let begin = Instant::now();
            let now = || u64::try_from(begin.elapsed().as_millis()).unwrap_or(u64::MAX);
            let mut sampler = Sampler::default();
            let mut next = begin + interval;
            loop {
                // Disconnected means the owner was dropped: end like a stop.
                let ended = !matches!(stopped.recv_timeout(poll), Err(RecvTimeoutError::Timeout));
                let at = now();
                for ev in source.events() {
                    sampler.event(&ev, at);
                }
                if ended || Instant::now() >= next {
                    sink(&sampler.sample(source.queue_depth(), source.rss_bytes()));
                    next = Instant::now() + interval;
                }
                if ended {
                    return;
                }
            }
        })
        .ok()?;
    Some(PerfFeed { stop, thread })
}

/// Shared handle so that both the end of `serve` and the shutdown thread can stop the feed.
pub type SharedFeed = Arc<Mutex<Option<PerfFeed>>>;

/// Stops the feed in `shared`, once.
pub fn stop_shared(shared: &SharedFeed) {
    let feed = shared.lock().unwrap_or_else(|p| p.into_inner()).take();
    if let Some(feed) = feed {
        feed.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    fn ev(id: JobId, kind: &str, status: JobStatus) -> JobEvent {
        JobEvent {
            id,
            kind: kind.into(),
            status,
        }
    }

    fn running() -> JobStatus {
        JobStatus::Running {
            progress: 0.0,
            message: String::new(),
        }
    }

    #[test]
    fn a_finished_job_is_timed_from_running_to_done() {
        let mut s = Sampler::default();
        let (a, b) = (JobId::new(), JobId::new());
        s.event(&ev(a, "transcribe", JobStatus::Queued), 0);
        s.event(&ev(a, "transcribe", running()), 100);
        s.event(&ev(b, "render", running()), 150);
        s.event(
            &ev(a, "transcribe", JobStatus::Done(serde_json::Value::Null)),
            400,
        );
        s.event(&ev(b, "render", JobStatus::Failed("x".into())), 450);
        let sample = s.sample(2, Some(10));
        assert_eq!(sample.queue_depth, Some(2));
        assert_eq!(sample.rss_bytes, Some(10));
        assert_eq!(
            sample.jobs,
            vec![
                JobTiming::new("transcribe", 300),
                JobTiming::new("render", 300)
            ]
        );
        // A new window starts empty.
        assert!(s.sample(0, None).jobs.is_empty());
    }

    #[test]
    fn a_job_cancelled_while_queued_and_an_unseen_finish_add_nothing() {
        let mut s = Sampler::default();
        let id = JobId::new();
        s.event(&ev(id, "a", JobStatus::Queued), 0);
        s.event(&ev(id, "a", JobStatus::Cancelled), 5);
        s.event(&ev(JobId::new(), "b", JobStatus::Cancelled), 9);
        assert!(s.sample(0, None).jobs.is_empty());
    }

    #[test]
    fn a_job_kind_that_is_not_a_plain_name_is_masked_and_the_burst_is_capped() {
        let mut s = Sampler::default();
        for i in 0..(MAX_JOBS + 10) {
            let id = JobId::new();
            let kind = if i == 0 { "/Users/a/secret" } else { "k" };
            s.event(&ev(id, kind, running()), 0);
            s.event(&ev(id, kind, JobStatus::Cancelled), 1);
        }
        let sample = s.sample(0, None);
        assert_eq!(sample.jobs.len(), MAX_JOBS);
        assert_eq!(sample.jobs[0].kind(), "invalid-name");
    }

    struct Fake {
        queued: u32,
        pending: Vec<JobEvent>,
    }

    impl Source for Fake {
        fn events(&mut self) -> Vec<JobEvent> {
            std::mem::take(&mut self.pending)
        }
        fn queue_depth(&self) -> u32 {
            self.queued
        }
        fn rss_bytes(&self) -> Option<u64> {
            Some(4096)
        }
    }

    #[test]
    fn stopping_the_thread_writes_one_last_sample_and_joins() {
        let seen = Arc::new(StdMutex::new(Vec::new()));
        let sink = seen.clone();
        let feed = spawn(
            Fake {
                queued: 3,
                pending: vec![],
            },
            move |s| sink.lock().unwrap().push(s.clone()),
            // Far longer than the test: only the stop can produce the sample.
            Duration::from_secs(3600),
            Duration::from_secs(3600),
        )
        .unwrap();
        feed.stop();
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].queue_depth, Some(3));
        assert_eq!(seen[0].rss_bytes, Some(4096));
    }

    #[test]
    fn a_stopped_feed_is_stopped_once_through_the_shared_handle() {
        let seen = Arc::new(StdMutex::new(0usize));
        let sink = seen.clone();
        let feed = spawn(
            Fake {
                queued: 0,
                pending: vec![],
            },
            move |_| *sink.lock().unwrap() += 1,
            Duration::from_secs(3600),
            Duration::from_secs(3600),
        );
        let shared: SharedFeed = Arc::new(Mutex::new(feed));
        stop_shared(&shared);
        stop_shared(&shared);
        assert_eq!(*seen.lock().unwrap(), 1);
    }
}
