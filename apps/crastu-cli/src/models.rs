//! `models list` and `models download <id> [--dir PATH]`: the human check of the model
//! downloader. Progress goes to stderr so that stdout stays clean for scripts.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crastu_models::{Cancel, Catalogue, Commercial, Fetch, Progress, Task, download_with};

pub(crate) const USAGE: &str = "models <list | download ID [--dir PATH]>";

/// Runs a `models` command. `args` start at `models`; `default_dir` is where downloads go
/// without `--dir`.
pub(crate) fn run(
    args: &[String],
    default_dir: impl FnOnce() -> Result<PathBuf, String>,
    fetcher: &dyn Fetch,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<(), String> {
    let catalogue = crastu_models::catalogue();
    match args.get(1).map(String::as_str) {
        Some("list") => out
            .write_all(list_text(&catalogue).as_bytes())
            .map_err(|e| e.to_string()),
        Some("download") => {
            let id = args
                .get(2)
                .filter(|a| !a.starts_with("--"))
                .ok_or_else(|| format!("usage: {USAGE}"))?;
            if catalogue.get(id).is_none() {
                return Err(unknown(&catalogue, id));
            }
            let dir = match super::flag(args, "--dir") {
                Some(d) => PathBuf::from(d),
                None => default_dir()?,
            };
            download(&catalogue, id, &dir, fetcher, out, err)
        }
        _ => Err(format!("usage: {USAGE}")),
    }
}

fn task_name(task: Task) -> &'static str {
    match task {
        Task::SpeechToText => "speech to text",
        Task::VoiceActivity => "voice activity",
        Task::TextToSpeech => "text to speech",
        Task::FaceDetection => "face detection",
        Task::Embedding => "embedding",
    }
}

pub(crate) fn bytes(n: u64) -> String {
    const MB: f64 = 1_048_576.0;
    if n >= 1_048_576 {
        format!("{:.1} MB", n as f64 / MB)
    } else {
        format!("{:.1} KB", n as f64 / 1024.0)
    }
}

fn list_text(catalogue: &Catalogue) -> String {
    let mut text = String::new();
    for e in catalogue.entries() {
        let commercial = match &e.commercial {
            Commercial::Allowed => "commercial use allowed",
            Commercial::Conditional { .. } => "conditional",
            Commercial::Denied => "denied",
        };
        let size = e.size_bytes.map(bytes).unwrap_or_else(|| "?".into());
        text.push_str(&format!(
            "{:<14} {:<14} {:<15} {:<4} {:>9}  {}\n",
            e.id,
            e.name,
            task_name(e.task),
            e.licence,
            size,
            commercial
        ));
    }
    text
}

fn progress_line(id: &str, p: Progress) -> String {
    match p.total {
        Some(t) if t > 0 => format!(
            "{id}: {} / {} ({}%)",
            bytes(p.done),
            bytes(t),
            p.done * 100 / t
        ),
        _ => format!("{id}: {}", bytes(p.done)),
    }
}

fn unknown(catalogue: &Catalogue, id: &str) -> String {
    let ids: Vec<&str> = catalogue.entries().iter().map(|e| e.id.as_str()).collect();
    format!(
        "no model {id:?} in the catalogue (have: {})",
        ids.join(", ")
    )
}

fn download(
    catalogue: &Catalogue,
    id: &str,
    dir: &Path,
    fetcher: &dyn Fetch,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<(), String> {
    let entry = catalogue.get(id).ok_or_else(|| {
        let ids: Vec<&str> = catalogue.entries().iter().map(|e| e.id.as_str()).collect();
        format!(
            "no model {id:?} in the catalogue (have: {})",
            ids.join(", ")
        )
    })?;
    let started = Instant::now();
    let mut last = Instant::now() - Duration::from_secs(1);
    let mut last_pct = u64::MAX;
    let path = download_with(entry, dir, fetcher, &Cancel::new(), &mut |p| {
        let pct = p.total.filter(|t| *t > 0).map_or(0, |t| p.done * 100 / t);
        let finished = p.total == Some(p.done);
        if finished || (pct != last_pct && last.elapsed() >= Duration::from_millis(200)) {
            let _ = write!(err, "\r{}", progress_line(&entry.id, p));
            last = Instant::now();
            last_pct = pct;
        }
    })
    .map_err(|e| {
        let _ = writeln!(err);
        e.to_string()
    })?;
    let _ = writeln!(
        err,
        "\n{}: done in {:.1}s, sha256 {} verified",
        entry.id,
        started.elapsed().as_secs_f64(),
        entry.sha256
    );
    writeln!(out, "{}", path.display()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use std::io::{self, Cursor};

    use crastu_models::FetchResponse;

    use super::*;

    struct Memory(Vec<u8>);
    impl Fetch for Memory {
        fn fetch(&self, _: &str, offset: u64) -> io::Result<FetchResponse> {
            Ok(FetchResponse {
                reader: Box::new(Cursor::new(self.0[offset as usize..].to_vec())),
                start: offset,
                total: Some(self.0.len() as u64),
            })
        }
    }

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    fn run_with(a: &[&str], fetcher: &dyn Fetch) -> (Result<(), String>, String, String) {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let r = run(
            &args(a),
            || Err("no default dir in tests".into()),
            fetcher,
            &mut out,
            &mut err,
        );
        (
            r,
            String::from_utf8(out).unwrap(),
            String::from_utf8(err).unwrap(),
        )
    }

    #[test]
    fn list_shows_every_catalogue_model_with_its_licence() {
        let (r, out, _) = run_with(&["models", "list"], &Memory(vec![]));
        r.unwrap();
        for id in ["whisper-small", "silero-vad", "yunet"] {
            let line = out.lines().find(|l| l.starts_with(id)).unwrap();
            assert!(
                line.contains("MIT") && line.contains("commercial use allowed"),
                "{line}"
            );
        }
        assert!(!out.contains("supertonic"));
    }

    #[test]
    fn download_verifies_and_reports_the_path_and_progress() {
        // The real catalogue pins real files, so a fake body must fail the checksum.
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path().to_str().unwrap();
        let (r, out, err) = run_with(
            &["models", "download", "silero-vad", "--dir", d],
            &Memory(vec![7; 2_327_524]),
        );
        let e = r.unwrap_err();
        assert!(e.contains("checksum mismatch"), "{e}");
        assert!(out.is_empty());
        assert!(err.contains("silero-vad"), "{err}");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn a_good_download_prints_the_final_path_and_the_verified_hash() {
        use sha2::{Digest, Sha256};
        let body = vec![3u8; 300_000];
        let sha: String = Sha256::digest(&body)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let json = format!(
            r#"[{{"id":"m","name":"M","task":"voice_activity","licence":"MIT",
            "licence_url":"https://e.org/l","commercial":{{"status":"allowed"}},
            "url":"https://e.org/m","file_name":"m.onnx","sha256":"{sha}",
            "size_bytes":300000,"attribution":null}}]"#
        );
        let catalogue = Catalogue::from_json(&json).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let (mut out, mut err) = (Vec::new(), Vec::new());
        download(
            &catalogue,
            "m",
            dir.path(),
            &Memory(body),
            &mut out,
            &mut err,
        )
        .unwrap();
        let out = String::from_utf8(out).unwrap();
        assert_eq!(out.trim(), dir.path().join("m.onnx").display().to_string());
        let err = String::from_utf8(err).unwrap();
        assert!(err.contains(&format!("sha256 {sha} verified")), "{err}");
        assert!(err.contains("100%"), "{err}");
    }

    #[test]
    fn download_of_an_unknown_id_names_the_known_ones() {
        let (r, _, _) = run_with(&["models", "download", "nope"], &Memory(vec![]));
        let e = r.unwrap_err();
        assert!(
            e.contains("whisper-small") && e.contains("silero-vad"),
            "{e}"
        );
    }

    #[test]
    fn usage_errors() {
        assert!(run_with(&["models"], &Memory(vec![])).0.is_err());
        assert!(
            run_with(&["models", "download"], &Memory(vec![]))
                .0
                .is_err()
        );
        assert!(
            run_with(&["models", "download", "--dir", "x"], &Memory(vec![]))
                .0
                .is_err()
        );
    }

    #[test]
    fn sizes_and_progress_lines_read_well() {
        assert_eq!(bytes(2_327_524), "2.2 MB");
        assert_eq!(bytes(232_589), "227.1 KB");
        let p = Progress {
            done: 51_200,
            total: Some(204_800),
        };
        assert_eq!(progress_line("m", p), "m: 50.0 KB / 200.0 KB (25%)");
    }
}
