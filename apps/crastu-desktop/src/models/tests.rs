use std::io::{self, Cursor, Read};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use crastu_models::{Commercial, FetchResponse, Task};
use sha2::{Digest, Sha256};

use super::*;

fn body() -> Vec<u8> {
    // Longer than the downloader's 64 KiB read buffer, so progress arrives in several steps.
    (0..300_000u32).map(|i| (i % 251) as u8).collect()
}

fn entry(id: &str, file: &str, body: &[u8]) -> ModelEntry {
    ModelEntry {
        id: id.into(),
        name: id.into(),
        task: Task::SpeechToText,
        licence: "MIT".into(),
        licence_url: "https://example.org/licence".into(),
        commercial: Commercial::Allowed,
        url: "https://example.org/model.bin".into(),
        file_name: file.into(),
        sha256: Sha256::digest(body)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
        size_bytes: Some(body.len() as u64),
        attribution: None,
        note: None,
    }
}

/// A server in memory. `hold` stops the body after its first chunk until `release` is set, so a
/// test can cancel a download that is half way. `opened` counts the requests.
struct Server {
    body: Vec<u8>,
    hold: bool,
    release: Arc<AtomicBool>,
    opened: Arc<AtomicUsize>,
    offsets: Arc<Mutex<Vec<u64>>>,
}

struct Held {
    first: Cursor<Vec<u8>>,
    rest: Cursor<Vec<u8>>,
    release: Arc<AtomicBool>,
}

impl Read for Held {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.first.read(buf)?;
        if n > 0 {
            return Ok(n);
        }
        while !self.release.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(2));
        }
        self.rest.read(buf)
    }
}

impl Fetch for Server {
    fn fetch(&self, _: &str, offset: u64) -> io::Result<FetchResponse> {
        self.opened.fetch_add(1, Ordering::SeqCst);
        self.offsets.lock().unwrap().push(offset);
        let tail = self.body[offset as usize..].to_vec();
        let reader: Box<dyn Read> = if self.hold {
            let cut = tail.len().min(70_000);
            Box::new(Held {
                first: Cursor::new(tail[..cut].to_vec()),
                rest: Cursor::new(tail[cut..].to_vec()),
                release: self.release.clone(),
            })
        } else {
            Box::new(Cursor::new(tail))
        };
        Ok(FetchResponse {
            reader,
            start: offset,
            total: Some(self.body.len() as u64),
        })
    }
}

struct Rig {
    store: ModelStore,
    dir: tempfile::TempDir,
    gate: NetGate,
    release: Arc<AtomicBool>,
    opened: Arc<AtomicUsize>,
    offsets: Arc<Mutex<Vec<u64>>>,
}

fn rig(hold: bool, serve: Vec<u8>) -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let gate = NetGate::new(false);
    let (release, opened, offsets) = (
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicUsize::new(0)),
        Arc::new(Mutex::new(Vec::new())),
    );
    let (r, o, of) = (release.clone(), opened.clone(), offsets.clone());
    let make: MakeFetch = Arc::new(move || {
        Box::new(Server {
            body: serve.clone(),
            hold,
            release: r.clone(),
            opened: o.clone(),
            offsets: of.clone(),
        })
    });
    let good = body();
    let catalogue = Catalogue::new(vec![
        entry("one", "one.onnx", &good),
        entry("two", "two.onnx", &good),
    ])
    .unwrap();
    let store = ModelStore::new(dir.path().join("models"), catalogue, gate.clone(), make);
    Rig {
        store,
        dir,
        gate,
        release,
        opened,
        offsets,
    }
}

/// Drains events until `done` is satisfied by the ones seen so far, or fails after five seconds.
fn collect_until(
    store: &ModelStore,
    done: impl Fn(&[DownloadEvent]) -> bool,
) -> Vec<DownloadEvent> {
    let mut seen = Vec::new();
    let end = Instant::now() + Duration::from_secs(5);
    while Instant::now() < end {
        seen.extend(store.events());
        if done(&seen) {
            return seen;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("timed out; events so far: {seen:?}");
}

fn ended(events: &[DownloadEvent]) -> bool {
    events.iter().any(|e| {
        matches!(
            e,
            DownloadEvent::Done { .. }
                | DownloadEvent::Failed { .. }
                | DownloadEvent::Cancelled { .. }
        )
    })
}

fn models_dir(r: &Rig) -> PathBuf {
    r.dir.path().join("models")
}

#[test]
fn a_download_reports_progress_then_done_and_the_model_is_installed() {
    let r = rig(false, body());
    assert!(
        r.store.installed().is_empty(),
        "nothing before the download"
    );
    r.store.start("one").unwrap();
    let events = collect_until(&r.store, ended);
    assert_eq!(
        events.last(),
        Some(&DownloadEvent::Done { id: "one".into() })
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, DownloadEvent::Progress { .. }))
    );
    assert_eq!(r.store.installed(), ["one"]);
    assert_eq!(fs::read(models_dir(&r).join("one.onnx")).unwrap(), body());
    assert!(!models_dir(&r).join("one.onnx.part").exists());
}

#[test]
fn progress_of_one_model_is_coalesced_to_the_newest() {
    let shared = Shared::default();
    for done in [1, 2, 3] {
        shared.push(DownloadEvent::Progress {
            id: "a".into(),
            done,
            total: Some(9),
        });
    }
    shared.push(DownloadEvent::Progress {
        id: "b".into(),
        done: 5,
        total: Some(9),
    });
    shared.push(DownloadEvent::Done { id: "a".into() });
    let events = std::mem::take(&mut *lock(&shared.events));
    assert_eq!(events.len(), 3);
    assert_eq!(
        events[0],
        DownloadEvent::Progress {
            id: "a".into(),
            done: 3,
            total: Some(9)
        }
    );
}

#[test]
fn cancel_stops_the_download_and_keeps_the_part_file_for_a_resume() {
    let r = rig(true, body());
    r.store.start("one").unwrap();
    // The first chunk has been written once a progress event shows bytes.
    collect_until(&r.store, |e| {
        e.iter()
            .any(|e| matches!(e, DownloadEvent::Progress { done, .. } if *done > 0))
    });
    r.store.cancel("one");
    r.release.store(true, Ordering::SeqCst);
    let events = collect_until(&r.store, ended);
    assert_eq!(
        events.last(),
        Some(&DownloadEvent::Cancelled { id: "one".into() })
    );
    assert!(r.store.installed().is_empty());
    let part = models_dir(&r).join("one.onnx.part");
    assert!(fs::metadata(&part).unwrap().len() > 0, "the part stays");
    assert!(!models_dir(&r).join("one.onnx").exists());
}

#[test]
fn a_new_download_resumes_the_part_file() {
    let r = rig(false, body());
    fs::create_dir_all(models_dir(&r)).unwrap();
    fs::write(models_dir(&r).join("one.onnx.part"), &body()[..100_000]).unwrap();
    r.store.start("one").unwrap();
    let events = collect_until(&r.store, ended);
    assert_eq!(
        events.last(),
        Some(&DownloadEvent::Done { id: "one".into() })
    );
    assert_eq!(*r.offsets.lock().unwrap(), [100_000]);
}

#[test]
fn offline_refuses_the_download_before_any_request_is_opened() {
    let r = rig(false, body());
    r.gate.set_offline(true);
    r.store.start("one").unwrap();
    let events = collect_until(&r.store, ended);
    match events.last() {
        Some(DownloadEvent::Failed { id, message }) => {
            assert_eq!(id, "one");
            assert!(message.contains("Offline"), "{message}");
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
    assert_eq!(r.opened.load(Ordering::SeqCst), 0, "no request was opened");
}

#[test]
fn a_bad_checksum_is_a_failure_and_leaves_no_model() {
    let mut wrong = body();
    wrong[10] ^= 0xff;
    let r = rig(false, wrong);
    r.store.start("one").unwrap();
    let events = collect_until(&r.store, ended);
    assert!(
        matches!(events.last(), Some(DownloadEvent::Failed { message, .. }) if message.contains("checksum")),
        "{events:?}"
    );
    assert!(r.store.installed().is_empty());
}

#[test]
fn a_model_cannot_be_started_twice_and_the_slot_frees_when_it_ends() {
    let r = rig(true, body());
    r.store.start("one").unwrap();
    assert!(r.store.start("one").unwrap_err().contains("already"));
    r.store.start("two").unwrap(); // another model is independent
    r.release.store(true, Ordering::SeqCst);
    collect_until(&r.store, |e| {
        e.iter()
            .filter(|e| matches!(e, DownloadEvent::Done { .. }))
            .count()
            == 2
    });
    assert_eq!(r.store.installed().len(), 2);
    // Done again: starting once more is allowed (the file is complete, nothing is fetched).
    r.store.start("one").unwrap();
    collect_until(&r.store, ended);
}

#[test]
fn an_unknown_id_is_refused_everywhere() {
    let r = rig(false, body());
    for id in ["../x", "nope", "", "one/../two"] {
        assert!(r.store.start(id).is_err(), "{id:?}");
        assert!(r.store.remove(id).is_err(), "{id:?}");
    }
    r.store.cancel("../x");
    assert_eq!(r.opened.load(Ordering::SeqCst), 0);
}

#[test]
fn remove_deletes_the_model_and_its_part_and_nothing_else() {
    let r = rig(false, body());
    let dir = models_dir(&r);
    fs::create_dir_all(&dir).unwrap();
    for name in ["one.onnx", "one.onnx.part", "two.onnx", "keep.txt"] {
        fs::write(dir.join(name), b"x").unwrap();
    }
    let outside = r.dir.path().join("one.onnx");
    fs::write(&outside, b"outside").unwrap();
    r.store.remove("one").unwrap();
    assert!(!dir.join("one.onnx").exists() && !dir.join("one.onnx.part").exists());
    for kept in ["two.onnx", "keep.txt"] {
        assert!(dir.join(kept).exists(), "{kept} stays");
    }
    assert!(outside.exists(), "a file outside the models folder stays");
    // Removing what is not there is fine.
    r.store.remove("one").unwrap();
}

#[test]
fn remove_is_refused_while_the_model_downloads() {
    let r = rig(true, body());
    r.store.start("one").unwrap();
    let err = r.store.remove("one").unwrap_err();
    assert!(err.contains("downloading"), "{err}");
    r.store.cancel("one");
    r.release.store(true, Ordering::SeqCst);
    collect_until(&r.store, ended);
}

#[test]
fn only_plain_file_names_pass_the_removal_check() {
    assert_eq!(plain_name("model.onnx"), Ok("model.onnx"));
    for bad in ["", "..", ".", "../x", "a/b", "/abs", "a\\b"] {
        // "a\\b" is one name on unix and two components on Windows; the catalogue rejects it
        // anyway, so only the cases that are wrong everywhere are asserted for it.
        if bad == "a\\b" && !cfg!(windows) {
            continue;
        }
        assert!(plain_name(bad).is_err(), "{bad:?}");
    }
}

struct Panics;

impl Fetch for Panics {
    fn fetch(&self, _: &str, _: u64) -> io::Result<FetchResponse> {
        panic!("the adapter broke");
    }
}

#[test]
fn a_panicking_adapter_ends_as_a_failed_row_and_frees_the_slot() {
    let dir = tempfile::tempdir().unwrap();
    let good = body();
    let catalogue = Catalogue::new(vec![entry("one", "one.onnx", &good)]).unwrap();
    let make: MakeFetch = Arc::new(|| Box::new(Panics));
    let store = ModelStore::new(
        dir.path().join("models"),
        catalogue,
        NetGate::new(false),
        make,
    );
    // The wait is only a failure guard: a hang fails the test instead of stalling it.
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut ends = Vec::new();
        for _ in 0..2 {
            // The second start only works when the first one freed its slot.
            store.start("one").expect("the slot is free");
            ends.push(collect_until(&store, ended).pop());
        }
        tx.send(ends).ok();
    });
    let ends = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("the download hung after a panic");
    for end in ends {
        match end {
            Some(DownloadEvent::Failed { id, message }) => {
                assert_eq!(id, "one");
                assert!(message.contains("unexpectedly"), "{message}");
            }
            other => panic!("expected a failed row, got {other:?}"),
        }
    }
}

#[test]
fn a_remove_racing_a_start_never_deletes_the_part_of_the_new_download() {
    for _ in 0..25 {
        let r = Arc::new(rig(true, body()));
        let part = models_dir(&r).join("one.onnx.part");
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let remover = {
            let (r, barrier) = (r.clone(), barrier.clone());
            std::thread::spawn(move || {
                barrier.wait();
                r.store.remove("one")
            })
        };
        barrier.wait();
        let started = r.store.start("one");
        let removed = remover.join().unwrap();
        if started.is_ok() {
            // Once bytes are on disk the part must stay until the download ends.
            collect_until(&r.store, |e| {
                e.iter()
                    .any(|e| matches!(e, DownloadEvent::Progress { done, .. } if *done > 0))
            });
            assert!(part.exists(), "remove deleted the running download's part");
            if removed.is_ok() {
                // Remove ran first and finished before the download began.
                assert!(r.store.remove("one").is_err());
            }
            r.store.cancel("one");
            r.release.store(true, Ordering::SeqCst);
            collect_until(&r.store, ended);
        }
    }
}

#[test]
fn remove_also_deletes_the_empty_part_lock() {
    let r = rig(false, body());
    r.store.start("one").unwrap();
    collect_until(&r.store, ended);
    let lock_file = models_dir(&r).join("one.onnx.part.lock");
    assert!(
        lock_file.exists(),
        "a finished download leaves its lock file"
    );
    r.store.remove("one").unwrap();
    assert!(!lock_file.exists());
}

#[test]
fn remove_and_start_take_the_same_lock_so_neither_runs_inside_the_other() {
    let r = Arc::new(rig(false, body()));
    let guard = lock(&r.store.shared.active);
    let (tx, rx) = std::sync::mpsc::channel();
    let remover = {
        let (r, tx) = (r.clone(), tx.clone());
        std::thread::spawn(move || {
            let result = r.store.remove("one");
            tx.send("remove").ok();
            result
        })
    };
    let starter = {
        let r = r.clone();
        std::thread::spawn(move || {
            let result = r.store.start("one");
            tx.send("start").ok();
            result
        })
    };
    // Both wait while the lock is held elsewhere.
    assert!(rx.recv_timeout(Duration::from_millis(200)).is_err());
    drop(guard);
    for _ in 0..2 {
        rx.recv_timeout(Duration::from_secs(5))
            .expect("a call stayed blocked");
    }
    // Either order is allowed, but the two never overlap: remove first (it succeeds, then the
    // download starts), or start first (the download is active, so remove is refused and does not
    // touch its files).
    let removed = remover.join().unwrap();
    match removed {
        Ok(()) => {}
        Err(e) => assert!(e.contains("downloading"), "{e}"),
    }
    starter.join().unwrap().unwrap();
    collect_until(&r.store, ended);
}
