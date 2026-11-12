use std::cell::RefCell;
use std::io::{self, Read};

use super::*;
use crate::{Commercial, Task};

fn body() -> Vec<u8> {
    // Longer than the 64 KiB read buffer so that the copy loop runs several times.
    (0..150_000u32).map(|i| (i % 251) as u8).collect()
}

fn sha(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex(h)
}

fn entry(body: &[u8]) -> ModelEntry {
    ModelEntry {
        id: "model".into(),
        name: "Model".into(),
        task: Task::SpeechToText,
        licence: "MIT".into(),
        licence_url: "https://example.org/licence".into(),
        commercial: Commercial::Allowed,
        url: "https://example.org/model.bin".into(),
        file_name: "model.onnx".into(),
        sha256: sha(body),
        size_bytes: Some(body.len() as u64),
        attribution: None,
        note: None,
    }
}

/// A server in memory: serves `body` from the requested offset, or from 0 when it ignores
/// ranges; can cut the connection after some bytes, add bytes, or announce another total.
struct Fake {
    calls: RefCell<Vec<u64>>,
    body: Vec<u8>,
    honour_range: bool,
    cut_after: Option<usize>,
    extra: usize,
    announce: Option<u64>,
}

impl Fake {
    fn new(body: &[u8]) -> Fake {
        Fake {
            calls: RefCell::new(vec![]),
            body: body.to_vec(),
            honour_range: true,
            cut_after: None,
            extra: 0,
            announce: Some(body.len() as u64),
        }
    }
}

struct Cut(io::Cursor<Vec<u8>>, usize);
impl Read for Cut {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.1 == 0 {
            return Err(io::Error::new(io::ErrorKind::ConnectionReset, "cut"));
        }
        let room = buf.len().min(self.1);
        let n = self.0.read(&mut buf[..room])?;
        self.1 -= n;
        Ok(n)
    }
}

impl Fetch for Fake {
    fn fetch(&self, _url: &str, offset: u64) -> io::Result<FetchResponse> {
        self.calls.borrow_mut().push(offset);
        let start = if self.honour_range { offset } else { 0 };
        let mut bytes = self.body[start as usize..].to_vec();
        bytes.extend(std::iter::repeat_n(0xAA, self.extra));
        let reader: Box<dyn Read> = match self.cut_after {
            Some(n) => Box::new(Cut(io::Cursor::new(bytes), n)),
            None => Box::new(io::Cursor::new(bytes)),
        };
        Ok(FetchResponse {
            reader,
            start,
            total: self.announce,
        })
    }
}

fn names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        // The empty lock file stays by design; it is not a download artefact.
        .filter(|n| !n.ends_with(".part.lock"))
        .collect();
    v.sort();
    v
}

#[test]
fn a_full_download_lands_under_its_final_name_with_progress() {
    let b = body();
    let dir = tempfile::tempdir().unwrap();
    let fake = Fake::new(&b);
    let mut seen = vec![];
    let path = download_with(&entry(&b), dir.path(), &fake, &Cancel::new(), &mut |p| {
        seen.push(p)
    })
    .unwrap();
    assert_eq!(path, dir.path().join("model.onnx"));
    assert_eq!(fs::read(&path).unwrap(), b);
    assert_eq!(names(dir.path()), ["model.onnx"], "no .part file is left");
    assert_eq!(*fake.calls.borrow(), vec![0]);
    assert_eq!(seen.first().unwrap().done, 0);
    assert_eq!(seen.last().unwrap().done, b.len() as u64);
    assert!(seen.windows(2).all(|w| w[0].done <= w[1].done));
    assert!(seen.len() > 3, "progress is reported per chunk");
    assert!(seen.iter().all(|p| p.total == Some(b.len() as u64)));
}

#[test]
fn a_cut_connection_keeps_the_part_and_the_next_call_resumes() {
    let b = body();
    let dir = tempfile::tempdir().unwrap();
    let mut fake = Fake::new(&b);
    fake.cut_after = Some(70_000);
    let err = download(&entry(&b), dir.path(), &fake).unwrap_err();
    assert!(matches!(err, ModelError::Io(_)), "{err}");
    assert_eq!(names(dir.path()), ["model.onnx.part"]);
    assert_eq!(
        fs::metadata(dir.path().join("model.onnx.part"))
            .unwrap()
            .len(),
        70_000
    );
    fake.cut_after = None;
    let path = download(&entry(&b), dir.path(), &fake).unwrap();
    assert_eq!(fs::read(path).unwrap(), b);
    assert_eq!(
        *fake.calls.borrow(),
        vec![0, 70_000],
        "second call asks for the rest"
    );
    assert_eq!(names(dir.path()), ["model.onnx"]);
}

#[test]
fn a_server_that_ignores_range_restarts_from_zero_without_duplicating_bytes() {
    let b = body();
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("model.onnx.part"), &b[..1000]).unwrap();
    let mut fake = Fake::new(&b);
    fake.honour_range = false;
    let path = download(&entry(&b), dir.path(), &fake).unwrap();
    assert_eq!(fs::read(path).unwrap(), b);
    assert_eq!(*fake.calls.borrow(), vec![1000]);
}

#[test]
fn a_resumed_part_that_does_not_belong_to_the_file_starts_over_once() {
    let b = body();
    let dir = tempfile::tempdir().unwrap();
    // The part is a prefix of an older version of the file.
    fs::write(dir.path().join("model.onnx.part"), vec![9u8; 5000]).unwrap();
    let fake = Fake::new(&b);
    let path = download(&entry(&b), dir.path(), &fake).unwrap();
    assert_eq!(fs::read(path).unwrap(), b);
    assert_eq!(*fake.calls.borrow(), vec![5000, 0]);
}

#[test]
fn a_bad_checksum_deletes_the_part_and_nothing_gets_the_final_name() {
    let b = body();
    let dir = tempfile::tempdir().unwrap();
    let mut e = entry(&b);
    e.sha256 = sha(b"something else");
    let fake = Fake::new(&b);
    let err = download(&e, dir.path(), &fake).unwrap_err();
    assert!(matches!(err, ModelError::Checksum { .. }), "{err}");
    assert!(names(dir.path()).is_empty(), "{:?}", names(dir.path()));
    assert_eq!(
        fake.calls.borrow().len(),
        1,
        "a fresh download is not repeated"
    );
}

#[test]
fn cancel_stops_early_and_keeps_the_part_for_a_later_resume() {
    let b = body();
    let dir = tempfile::tempdir().unwrap();
    let fake = Fake::new(&b);
    let cancel = Cancel::new();
    let err = download_with(&entry(&b), dir.path(), &fake, &cancel, &mut |p| {
        if p.done >= 65_536 {
            cancel.cancel();
        }
    })
    .unwrap_err();
    assert!(matches!(err, ModelError::Cancelled), "{err}");
    let part = fs::metadata(dir.path().join("model.onnx.part"))
        .unwrap()
        .len();
    assert!(
        (65_536..b.len() as u64).contains(&part),
        "stopped at {part}"
    );
    assert!(!dir.path().join("model.onnx").exists());
    let path = download(&entry(&b), dir.path(), &fake).unwrap();
    assert_eq!(fs::read(path).unwrap(), b);
    assert_eq!(*fake.calls.borrow(), vec![0, part]);
}

#[test]
fn fewer_bytes_than_announced_is_an_error_and_the_part_stays() {
    let b = body();
    let dir = tempfile::tempdir().unwrap();
    let mut fake = Fake::new(&b);
    fake.body.truncate(20_000);
    let err = download(&entry(&b), dir.path(), &fake).unwrap_err();
    assert!(
        matches!(
            err,
            ModelError::Size {
                expected: 150_000,
                actual: 20_000,
                ..
            }
        ),
        "{err}"
    );
    assert_eq!(names(dir.path()), ["model.onnx.part"]);
}

#[test]
fn more_bytes_than_announced_is_an_error_and_the_part_is_deleted() {
    let b = body();
    let dir = tempfile::tempdir().unwrap();
    let mut fake = Fake::new(&b);
    fake.extra = 10;
    let err = download(&entry(&b), dir.path(), &fake).unwrap_err();
    assert!(
        matches!(
            err,
            ModelError::Size {
                expected: 150_000,
                ..
            }
        ),
        "{err}"
    );
    assert!(names(dir.path()).is_empty(), "{:?}", names(dir.path()));
}

#[test]
fn a_server_that_announces_another_size_is_refused() {
    let b = body();
    let dir = tempfile::tempdir().unwrap();
    let mut fake = Fake::new(&b);
    fake.announce = Some(b.len() as u64 + 1);
    let err = download(&entry(&b), dir.path(), &fake).unwrap_err();
    assert!(matches!(err, ModelError::Size { .. }), "{err}");
    assert!(names(dir.path()).is_empty());
}

#[test]
fn a_part_that_is_already_complete_is_verified_without_a_request() {
    let b = body();
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("model.onnx.part"), &b).unwrap();
    let fake = Fake::new(&b);
    let path = download(&entry(&b), dir.path(), &fake).unwrap();
    assert_eq!(fs::read(path).unwrap(), b);
    assert!(fake.calls.borrow().is_empty());
}

#[test]
fn a_part_longer_than_the_file_is_dropped_and_a_good_target_is_kept() {
    let b = body();
    let dir = tempfile::tempdir().unwrap();
    let mut long = b.clone();
    long.push(1);
    fs::write(dir.path().join("model.onnx.part"), &long).unwrap();
    let fake = Fake::new(&b);
    let path = download(&entry(&b), dir.path(), &fake).unwrap();
    assert_eq!(fs::read(&path).unwrap(), b);
    assert_eq!(*fake.calls.borrow(), vec![0]);
    download(&entry(&b), dir.path(), &fake).unwrap();
    assert_eq!(
        fake.calls.borrow().len(),
        1,
        "a complete file is not fetched again"
    );
}

#[test]
fn a_body_that_starts_somewhere_else_is_a_protocol_error() {
    struct Wrong;
    impl Fetch for Wrong {
        fn fetch(&self, _: &str, _: u64) -> io::Result<FetchResponse> {
            Ok(FetchResponse {
                reader: Box::new(io::empty()),
                start: 7,
                total: None,
            })
        }
    }
    let b = body();
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("model.onnx.part"), &b[..100]).unwrap();
    let err = download(&entry(&b), dir.path(), &Wrong).unwrap_err();
    assert!(matches!(err, ModelError::Protocol { .. }), "{err}");
}

#[test]
fn a_second_download_of_the_same_model_is_refused_while_the_first_runs() {
    let b = body();
    let dir = tempfile::tempdir().unwrap();
    let fake = Fake::new(&b);
    let mut inner = None;
    let first = download_with(&entry(&b), dir.path(), &fake, &Cancel::new(), &mut |p| {
        if p.done > 0 && inner.is_none() {
            inner = Some(download(&entry(&b), dir.path(), &Fake::new(&b)));
        }
    });
    assert_eq!(
        fs::read(first.unwrap()).unwrap(),
        b,
        "the first download is not disturbed"
    );
    let err = inner.unwrap().unwrap_err();
    assert!(matches!(err, ModelError::Locked { .. }), "{err}");
    let message = err.to_string();
    assert!(message.contains("already downloading"), "{message}");
    assert!(message.contains("model.onnx.part.lock"), "{message}");
}

#[test]
fn a_leftover_lock_file_from_a_dead_process_does_not_block() {
    let b = body();
    let dir = tempfile::tempdir().unwrap();
    // The file exists but nobody holds a lock on it: that is what a killed process leaves.
    fs::write(dir.path().join("model.onnx.part.lock"), "").unwrap();
    let fake = Fake::new(&b);
    let path = download(&entry(&b), dir.path(), &fake).unwrap();
    assert_eq!(fs::read(path).unwrap(), b);
}

#[test]
fn a_live_holder_blocks_the_download_and_releasing_it_unblocks() {
    let b = body();
    let dir = tempfile::tempdir().unwrap();
    let e = entry(&b);
    let held = PartLock::take(&e, dir.path()).unwrap();
    let fake = Fake::new(&b);
    let err = download(&e, dir.path(), &fake).unwrap_err();
    assert!(matches!(err, ModelError::Locked { .. }), "{err}");
    assert!(err.to_string().contains("model.onnx.part.lock"), "{err}");
    assert!(fake.calls.borrow().is_empty(), "no request while locked");
    drop(held);
    download(&e, dir.path(), &fake).unwrap();
}

/// Run by `a_lock_held_by_another_process_blocks_and_dies_with_it` in a child process: takes
/// the lock on the folder named by the environment, says so, and waits to be killed.
#[test]
fn lock_holder_child() {
    let Some(dir) = std::env::var_os("CRASTU_TEST_LOCK_DIR") else {
        return;
    };
    let b = body();
    let _held = PartLock::take(&entry(&b), Path::new(&dir)).unwrap();
    println!("locked");
    std::thread::sleep(std::time::Duration::from_secs(60));
}

#[test]
fn a_lock_held_by_another_process_blocks_and_dies_with_it() {
    use std::io::BufRead;
    use std::process::{Command, Stdio};
    let b = body();
    let dir = tempfile::tempdir().unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "download::tests::lock_holder_child",
            "--nocapture",
        ])
        .env("CRASTU_TEST_LOCK_DIR", dir.path())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut lines = io::BufReader::new(child.stdout.take().unwrap()).lines();
    while lines.next().unwrap().unwrap() != "locked" {}
    let fake = Fake::new(&b);
    let err = download(&entry(&b), dir.path(), &fake).unwrap_err();
    assert!(matches!(err, ModelError::Locked { .. }), "{err}");
    // A kill skips every destructor; the system must still release the lock.
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(dir.path().join("model.onnx.part.lock").exists());
    let path = download(&entry(&b), dir.path(), &fake).unwrap();
    assert_eq!(fs::read(path).unwrap(), b);
}

#[test]
fn a_good_file_finished_by_another_process_is_not_downloaded_over() {
    let b = body();
    let dir = tempfile::tempdir().unwrap();
    let e = entry(&b);
    download(&e, dir.path(), &Fake::new(&b)).unwrap();
    // A second process that only now reaches the lock finds the finished file under it.
    struct Never;
    impl Fetch for Never {
        fn fetch(&self, _: &str, _: u64) -> io::Result<FetchResponse> {
            panic!("a good file must not be fetched again");
        }
    }
    let path = download(&e, dir.path(), &Never).unwrap();
    assert_eq!(fs::read(path).unwrap(), b);
}

#[test]
fn the_lock_is_free_after_success_and_after_every_kind_of_error() {
    let b = body();
    let dir = tempfile::tempdir().unwrap();
    let e = entry(&b);
    let free = || drop(PartLock::take(&e, dir.path()).expect("the lock is free"));
    let mut bad = entry(&b);
    bad.sha256 = sha(b"other");
    assert!(download(&bad, dir.path(), &Fake::new(&b)).is_err());
    free();
    let mut cut = Fake::new(&b);
    cut.cut_after = Some(1000);
    assert!(download(&e, dir.path(), &cut).is_err());
    free();
    let cancel = Cancel::new();
    cancel.cancel();
    let err = download_with(&e, dir.path(), &Fake::new(&b), &cancel, &mut |_| {});
    assert!(matches!(err, Err(ModelError::Cancelled)));
    free();
    download(&e, dir.path(), &Fake::new(&b)).unwrap();
    free();
}
