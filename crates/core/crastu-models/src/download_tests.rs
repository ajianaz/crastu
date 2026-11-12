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
