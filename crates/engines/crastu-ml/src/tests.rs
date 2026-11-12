use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

use crate::*;

const IDLE: Duration = Duration::from_secs(60);

struct FakeSession {
    id: usize,
    dropped: Arc<AtomicUsize>,
}

impl Drop for FakeSession {
    fn drop(&mut self) {
        self.dropped.fetch_add(1, Ordering::SeqCst);
    }
}

#[derive(Default)]
struct FakeFactory {
    created: AtomicUsize,
    dropped: Arc<AtomicUsize>,
    /// Number of upcoming creates that fail.
    fail: AtomicUsize,
    /// Creates sleep this long, so concurrent loads overlap.
    slow: Option<Duration>,
    /// Creates that have started.
    entered: AtomicUsize,
}

impl SessionFactory for Arc<FakeFactory> {
    type Session = FakeSession;

    fn create(&self, model: &ModelId, _o: &SessionOptions) -> Result<FakeSession, SessionError> {
        self.entered.fetch_add(1, Ordering::SeqCst);
        if let Some(delay) = self.slow.filter(|_| !model.as_str().starts_with("fast")) {
            thread::sleep(delay);
        }
        if self.fail.load(Ordering::SeqCst) > 0 {
            self.fail.fetch_sub(1, Ordering::SeqCst);
            return Err(SessionError::new(model, "boom"));
        }
        let id = self.created.fetch_add(1, Ordering::SeqCst) + 1;
        Ok(FakeSession {
            id,
            dropped: Arc::clone(&self.dropped),
        })
    }

    fn approx_memory(&self, _s: &FakeSession) -> Option<u64> {
        Some(100)
    }
}

fn setup(
    factory: FakeFactory,
) -> (
    Registry<Arc<FakeFactory>>,
    Arc<FakeFactory>,
    Arc<ManualClock>,
) {
    let factory = Arc::new(factory);
    let clock = Arc::new(ManualClock::default());
    let registry = Registry::with_clock(Arc::clone(&factory), IDLE, clock.clone());
    (registry, factory, clock)
}

fn model(name: &str) -> ModelId {
    ModelId::new(name)
}

#[test]
fn two_handles_share_one_session() {
    let (reg, factory, _) = setup(FakeFactory::default());
    let a = reg.load(&model("m"), SessionOptions::default()).unwrap();
    let b = reg.load(&model("m"), SessionOptions::default()).unwrap();
    assert_eq!(a.id, b.id);
    assert_eq!(factory.created.load(Ordering::SeqCst), 1);
    let snap = reg.snapshot();
    assert_eq!(snap.session_count(), 1);
    assert_eq!(snap.handles_for(&model("m")), 2);
    assert_eq!(snap.approx_memory(), Some(100));
}

#[test]
fn session_is_freed_after_both_handles_drop_and_idle_elapses() {
    let (reg, factory, clock) = setup(FakeFactory::default());
    let a = reg.load(&model("m"), SessionOptions::default()).unwrap();
    let b = a.clone();
    drop(a);
    clock.advance(IDLE * 2);
    // One handle is still alive: never freed, however long it takes.
    assert_eq!(reg.sweep(), 0);
    assert_eq!(reg.snapshot().session_count(), 1);
    drop(b);
    clock.advance(IDLE - Duration::from_secs(1));
    assert_eq!(reg.sweep(), 0, "not yet idle long enough");
    assert_eq!(factory.dropped.load(Ordering::SeqCst), 0);
    clock.advance(Duration::from_secs(1));
    assert_eq!(reg.sweep(), 1);
    assert_eq!(factory.dropped.load(Ordering::SeqCst), 1);
    assert_eq!(reg.snapshot().session_count(), 0);
}

#[test]
fn load_within_idle_window_reuses_and_cancels_expiry() {
    let (reg, factory, clock) = setup(FakeFactory::default());
    let first = reg.load(&model("m"), SessionOptions::default()).unwrap().id;
    clock.advance(IDLE - Duration::from_secs(1));
    let again = reg.load(&model("m"), SessionOptions::default()).unwrap();
    assert_eq!(again.id, first);
    assert_eq!(factory.created.load(Ordering::SeqCst), 1);
    // Held again: the old idle timer must not free it.
    clock.advance(IDLE * 3);
    assert_eq!(reg.sweep(), 0);
    drop(again);
    // The timer restarts from the new drop.
    clock.advance(IDLE - Duration::from_secs(1));
    assert_eq!(reg.sweep(), 0);
}

#[test]
fn load_after_idle_creates_a_new_session() {
    let (reg, factory, clock) = setup(FakeFactory::default());
    drop(reg.load(&model("m"), SessionOptions::default()).unwrap());
    clock.advance(IDLE);
    let again = reg.load(&model("m"), SessionOptions::default()).unwrap();
    assert_eq!(again.id, 2);
    assert_eq!(factory.dropped.load(Ordering::SeqCst), 1);
}

#[test]
fn zero_idle_frees_on_last_drop() {
    let factory = Arc::new(FakeFactory::default());
    let reg = Registry::new(Arc::clone(&factory), Duration::ZERO);
    drop(reg.load(&model("m"), SessionOptions::default()).unwrap());
    assert_eq!(factory.dropped.load(Ordering::SeqCst), 1);
    assert_eq!(reg.snapshot().session_count(), 0);
}

#[test]
fn concurrent_loads_of_one_key_create_one_session() {
    const N: usize = 8;
    let factory = FakeFactory {
        slow: Some(Duration::from_millis(200)),
        ..FakeFactory::default()
    };
    let (reg, factory, _) = setup(factory);
    let start = Arc::new(Barrier::new(N));
    let threads: Vec<_> = (0..N)
        .map(|_| {
            let (reg, start) = (reg.clone(), Arc::clone(&start));
            thread::spawn(move || {
                start.wait();
                reg.load(&model("m"), SessionOptions::default()).unwrap().id
            })
        })
        .collect();
    let ids: HashSet<usize> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    assert_eq!(ids, HashSet::from([1]));
    assert_eq!(factory.created.load(Ordering::SeqCst), 1);
    assert_eq!(factory.entered.load(Ordering::SeqCst), 1);
}

#[test]
fn slow_create_does_not_block_other_keys() {
    let factory = FakeFactory {
        slow: Some(Duration::from_millis(300)),
        ..FakeFactory::default()
    };
    let (reg, factory, _) = setup(factory);
    let slow = {
        let reg = reg.clone();
        thread::spawn(move || {
            reg.load(&model("slow"), SessionOptions::default())
                .unwrap()
                .id
        })
    };
    while factory.entered.load(Ordering::SeqCst) == 0 {
        thread::yield_now();
    }
    // The slow create is inside the factory; other keys, snapshot and sweep must not wait for it.
    let started = std::time::Instant::now();
    assert_eq!(
        reg.load(&model("fast-key"), SessionOptions::default())
            .unwrap()
            .id,
        1
    );
    assert_eq!(reg.snapshot().session_count(), 1);
    assert_eq!(reg.sweep(), 0);
    assert!(
        started.elapsed() < Duration::from_millis(250),
        "blocked behind the slow create"
    );
    assert_eq!(slow.join().unwrap(), 2);
}

#[test]
fn failed_create_is_not_cached() {
    let factory = FakeFactory::default();
    factory.fail.store(1, Ordering::SeqCst);
    let (reg, factory, _) = setup(factory);
    let err = reg
        .load(&model("m"), SessionOptions::default())
        .unwrap_err();
    assert_eq!(err.reason, "boom");
    assert_eq!(reg.snapshot().session_count(), 0);
    let ok = reg.load(&model("m"), SessionOptions::default()).unwrap();
    assert_eq!(ok.id, 1);
    assert_eq!(factory.created.load(Ordering::SeqCst), 1);
}

struct PanicFactory(AtomicUsize);

impl SessionFactory for PanicFactory {
    type Session = usize;

    fn create(&self, _m: &ModelId, _o: &SessionOptions) -> Result<usize, SessionError> {
        assert!(
            self.0.fetch_add(1, Ordering::SeqCst) > 0,
            "first create panics"
        );
        Ok(7)
    }
}

#[test]
fn panicking_create_does_not_poison_later_loads() {
    let reg = Registry::new(PanicFactory(AtomicUsize::new(0)), IDLE);
    let first = {
        let reg = reg.clone();
        thread::spawn(move || reg.load(&model("m"), SessionOptions::default()).map(|h| *h))
    };
    assert!(first.join().is_err());
    assert_eq!(
        *reg.load(&model("m"), SessionOptions::default()).unwrap(),
        7
    );
}

#[test]
fn different_options_are_different_sessions() {
    let (reg, factory, _) = setup(FakeFactory::default());
    let cpu = SessionOptions::default().with_intra_threads(2);
    let a = reg.load(&model("m"), cpu).unwrap();
    let b = reg.load(&model("m"), cpu.with_intra_threads(3)).unwrap();
    let c = reg
        .load(&model("m"), cpu.with_provider(ExecutionProvider::CoreMl))
        .unwrap();
    let d = reg
        .load(
            &model("m"),
            cpu.with_graph_optimization(GraphOptimization::Basic),
        )
        .unwrap();
    let e = reg.load(&model("other"), cpu).unwrap();
    let ids: HashSet<usize> = [&a, &b, &c, &d, &e].iter().map(|h| h.id).collect();
    assert_eq!(ids.len(), 5);
    assert_eq!(factory.created.load(Ordering::SeqCst), 5);
    let snap = reg.snapshot();
    assert_eq!(snap.session_count(), 5);
    assert_eq!(snap.handles_for(&model("m")), 4);
    assert_eq!(snap.total_handles(), 5);
    assert_eq!(snap.approx_memory(), Some(500));
}

#[test]
fn options_defaults_and_determinism() {
    let o = SessionOptions::default();
    assert_eq!(o.provider, ExecutionProvider::Cpu);
    assert!(o.is_deterministic());
    assert!((1..=4).contains(&o.intra_threads));
    assert_eq!(o.intra_threads, default_intra_threads());
    assert!(
        !o.with_provider(ExecutionProvider::CoreMl)
            .is_deterministic()
    );
    assert_eq!(o.with_intra_threads(0).intra_threads, 1);
}

#[test]
fn registry_and_handle_are_send_and_sync() {
    fn check<T: Send + Sync>() {}
    check::<Registry<Arc<FakeFactory>>>();
    check::<Handle<Arc<FakeFactory>>>();
}
