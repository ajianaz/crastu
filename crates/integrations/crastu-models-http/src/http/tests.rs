//! The real adapter against a local `TcpListener` HTTP server.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

use crastu_models::{
    Cancel, Commercial, Fetch, ModelEntry, ModelError, Task, download, download_with,
};
use sha2::{Digest, Sha256};

use super::*;

/// What the server does.
#[derive(Clone, Copy, Default)]
struct Behaviour {
    /// Answer 200 with the whole file even when a Range header is sent.
    ignore_range: bool,
    /// On the first connection only: send this many body bytes, then hang up.
    cut_first_after: Option<usize>,
}

struct Server {
    url: String,
    /// The Range header of every request, in order.
    ranges: Arc<Mutex<Vec<Option<String>>>>,
}

fn serve(body: Vec<u8>, behaviour: Behaviour) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/model.bin", listener.local_addr().unwrap());
    let ranges = Arc::new(Mutex::new(Vec::new()));
    let seen = ranges.clone();
    std::thread::spawn(move || {
        for (n, stream) in listener.incoming().enumerate() {
            let Ok(stream) = stream else { return };
            let cut = if n == 0 {
                behaviour.cut_first_after
            } else {
                None
            };
            let _ = handle(stream, &body, behaviour, cut, &seen);
        }
    });
    Server { url, ranges }
}

fn handle(
    mut stream: TcpStream,
    body: &[u8],
    behaviour: Behaviour,
    cut: Option<usize>,
    seen: &Mutex<Vec<Option<String>>>,
) -> std::io::Result<()> {
    let mut request = Vec::new();
    let mut byte = [0u8; 1];
    while !request.ends_with(b"\r\n\r\n") {
        if stream.read(&mut byte)? == 0 {
            return Ok(());
        }
        request.push(byte[0]);
    }
    let text = String::from_utf8_lossy(&request).into_owned();
    let range = text.lines().find_map(|l| {
        l.to_ascii_lowercase()
            .starts_with("range:")
            .then(|| l.split_once(':').unwrap().1.trim().to_string())
    });
    seen.lock().unwrap().push(range.clone());
    let total = body.len();
    let start = match (&range, behaviour.ignore_range) {
        (Some(r), false) => r
            .strip_prefix("bytes=")
            .and_then(|r| r.strip_suffix('-'))
            .and_then(|n| n.parse::<usize>().ok())
            .unwrap_or(0),
        _ => 0,
    };
    let head = if start > 0 {
        format!(
            "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {start}-{}/{total}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            total - 1,
            total - start
        )
    } else {
        format!("HTTP/1.1 200 OK\r\nContent-Length: {total}\r\nConnection: close\r\n\r\n")
    };
    stream.write_all(head.as_bytes())?;
    let payload = &body[start..];
    let payload = match cut {
        Some(n) => &payload[..n.min(payload.len())],
        None => payload,
    };
    stream.write_all(payload)?;
    stream.flush()
}

fn entry(body: &[u8], url: &str) -> ModelEntry {
    ModelEntry {
        id: "model".into(),
        name: "Model".into(),
        task: Task::VoiceActivity,
        licence: "MIT".into(),
        licence_url: "https://example.org/licence".into(),
        commercial: Commercial::Allowed,
        url: url.into(),
        file_name: "model.bin".into(),
        sha256: Sha256::digest(body)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
        size_bytes: Some(body.len() as u64),
        attribution: None,
        note: None,
    }
}

/// 11 MB: past the 10 MB default limit that ureq puts on `read_to_string`, to show that the
/// streaming reader has none.
fn big() -> Vec<u8> {
    (0..11_000_000u32).map(|i| (i % 253) as u8).collect()
}

#[test]
fn a_full_download_over_http() {
    let body = big();
    let server = serve(body.clone(), Behaviour::default());
    let dir = tempfile::tempdir().unwrap();
    let path = download(
        &entry(&body, &server.url),
        dir.path(),
        &HttpFetch::plain_http(),
    )
    .unwrap();
    assert_eq!(std::fs::read(path).unwrap(), body);
    assert_eq!(
        *server.ranges.lock().unwrap(),
        vec![None],
        "no Range on a fresh download"
    );
}

#[test]
fn a_cut_connection_resumes_with_a_range_request() {
    let body = big();
    let server = serve(
        body.clone(),
        Behaviour {
            cut_first_after: Some(3_000_000),
            ..Behaviour::default()
        },
    );
    let dir = tempfile::tempdir().unwrap();
    let e = entry(&body, &server.url);
    let http = HttpFetch::plain_http();
    let err = download(&e, dir.path(), &http).unwrap_err();
    assert!(
        matches!(err, ModelError::Io(_) | ModelError::Size { .. }),
        "{err}"
    );
    let part = std::fs::metadata(dir.path().join("model.bin.part"))
        .unwrap()
        .len();
    assert!(part > 0 && part <= 3_000_000, "kept {part} bytes");
    let path = download(&e, dir.path(), &http).unwrap();
    assert_eq!(std::fs::read(path).unwrap(), body);
    assert_eq!(
        *server.ranges.lock().unwrap(),
        vec![None, Some(format!("bytes={part}-"))]
    );
}

#[test]
fn a_server_that_ignores_range_is_handled() {
    let body: Vec<u8> = (0..300_000u32).map(|i| (i % 7) as u8).collect();
    let server = serve(
        body.clone(),
        Behaviour {
            ignore_range: true,
            ..Behaviour::default()
        },
    );
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("model.bin.part"), &body[..1234]).unwrap();
    let path = download(
        &entry(&body, &server.url),
        dir.path(),
        &HttpFetch::plain_http(),
    )
    .unwrap();
    assert_eq!(std::fs::read(path).unwrap(), body);
    assert_eq!(
        *server.ranges.lock().unwrap(),
        vec![Some("bytes=1234-".to_string())]
    );
}

#[test]
fn cancel_over_http_keeps_the_part() {
    let body: Vec<u8> = (0..400_000u32).map(|i| (i % 11) as u8).collect();
    let server = serve(body.clone(), Behaviour::default());
    let dir = tempfile::tempdir().unwrap();
    let cancel = Cancel::new();
    let err = download_with(
        &entry(&body, &server.url),
        dir.path(),
        &HttpFetch::plain_http(),
        &cancel,
        &mut |p| {
            if p.done > 0 {
                cancel.cancel();
            }
        },
    )
    .unwrap_err();
    assert!(matches!(err, ModelError::Cancelled), "{err}");
    assert!(dir.path().join("model.bin.part").exists());
    assert!(!dir.path().join("model.bin").exists());
}

#[test]
fn the_default_client_refuses_plain_http_without_connecting() {
    // Port 9 would refuse the connection; the error must come from the scheme check instead.
    let err = HttpFetch::new()
        .fetch("http://127.0.0.1:9/x", 0)
        .unwrap_err();
    assert!(err.to_string().to_lowercase().contains("http"), "{err}");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/x", listener.local_addr().unwrap());
    assert!(HttpFetch::new().fetch(&url, 0).is_err());
    assert!(listener.accept().is_err(), "no connection was made");
}

#[test]
fn content_range_is_parsed() {
    assert_eq!(
        parse_content_range("bytes 100-199/200"),
        Some((100, Some(200)))
    );
    assert_eq!(parse_content_range("bytes 5-9/*"), Some((5, None)));
    assert_eq!(parse_content_range("items 1-2/3"), None);
    assert_eq!(parse_content_range("bytes x-2/3"), None);
}
