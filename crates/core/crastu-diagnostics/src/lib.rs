//! The text behind Settings > About > "Copy diagnostics": a short report the user can paste into
//! an issue. Nothing is sent anywhere; the user decides to share it.
//!
//! It holds the version, the OS, the enabled modules, the last event lines of this run and the
//! first lines of the `end.json`, `panic.txt` and `crash.txt` of the run before. Every line read
//! from a file goes through `crastu-redact` again (the files were redacted when written, this is
//! the second net) and is cut to a length limit, and the whole report is bounded, so a huge or
//! odd file cannot make a huge or leaky report. The user's home folder is written as `~` in every
//! line (a source path from a dependency, a log message with a path), so the report does not carry
//! the account name; the report never lists the log folder itself and reads only the headers of
//! the files, not a backtrace.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crastu_logfiles::is_session_name;
use crastu_redact::{clean, ident};

/// Event lines taken from the end of this run's `events.jsonl`.
pub const EVENT_LINES: usize = 40;
/// Lines taken from the top of `end.json`, `panic.txt` and `crash.txt`: the reason comes first.
pub const HEADER_LINES: usize = 12;
/// A line longer than this is cut.
const LINE_CHARS: usize = 300;
/// The report never grows past this many bytes.
pub const MAX_BYTES: usize = 24 * 1024;
/// How much of the end of `events.jsonl` and of the start of a header file is read.
const READ_BYTES: u64 = 64 * 1024;
const HEADER_FILES: [&str; 3] = ["end.json", "panic.txt", "crash.txt"];

/// What the report is about.
#[derive(Clone, Copy, Debug)]
pub struct Diagnostics<'a> {
    pub app: &'a str,
    pub version: &'a str,
    pub os: &'a str,
    pub modules: &'a [String],
    /// This run's session folder, whose last events are included.
    pub current: Option<&'a Path>,
    /// The folder that holds this app's sessions, to find the run before this one.
    pub app_dir: Option<&'a Path>,
    /// The user's home folders (`HOME`, `USERPROFILE` and the like), each written as `~` wherever
    /// it appears in a line.
    pub homes: &'a [String],
}

/// Builds the report. Never fails: what cannot be read is said to be missing.
pub fn build(d: &Diagnostics<'_>) -> String {
    let mut out = String::from("Crastu diagnostics\n");
    out.push_str(&format!("app: {}\n", ident(d.app)));
    out.push_str(&format!(
        "version: {}\n",
        clean(&scrub(d.version, d.homes), 64)
    ));
    out.push_str(&format!("os: {}\n", clean(d.os, 64)));
    let modules: Vec<String> = d.modules.iter().map(|m| ident(m)).collect();
    out.push_str(&format!(
        "modules: {}\n",
        if modules.is_empty() {
            "none".to_string()
        } else {
            modules.join(", ")
        }
    ));

    out.push_str("\nlast events of this run:\n");
    let events = d
        .current
        .map(|c| tail_lines(&c.join("events.jsonl"), EVENT_LINES));
    match events.as_deref() {
        Some([_, ..]) => {
            for line in events.iter().flatten() {
                out.push_str(&clean(&scrub(line, d.homes), LINE_CHARS));
                out.push('\n');
            }
        }
        _ => out.push_str("none\n"),
    }

    match d.app_dir.and_then(|a| previous_session(a, d.current)) {
        None => out.push_str("\nprevious run: none found\n"),
        Some(prev) => {
            let name = prev
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            out.push_str(&format!("\nprevious run: {name}\n"));
            for file in HEADER_FILES {
                out.push_str(&format!("{file}:\n"));
                match head_lines(&prev.join(file), HEADER_LINES) {
                    Some(lines) if !lines.is_empty() => {
                        for l in lines {
                            out.push_str("  ");
                            out.push_str(&clean(&scrub(&l, d.homes), LINE_CHARS));
                            out.push('\n');
                        }
                    }
                    _ => out.push_str("  missing or empty\n"),
                }
            }
        }
    }
    bound(out)
}

/// The user's home folders as the environment names them (`HOME`, `USERPROFILE`, and Windows'
/// `HOMEDRIVE` plus `HOMEPATH`), without duplicates: any of them may be the one in a path.
pub fn home_folders(get: impl Fn(&str) -> Option<String>) -> Vec<String> {
    let mut homes: Vec<String> = ["HOME", "USERPROFILE"]
        .iter()
        .filter_map(|k| get(k))
        .collect();
    if let (Some(drive), Some(path)) = (get("HOMEDRIVE"), get("HOMEPATH")) {
        homes.push(format!("{drive}{path}"));
    }
    homes.retain(|h| !h.trim().is_empty());
    homes.dedup();
    homes
}

/// `text` with each home folder written as `~`: in the form it was given, with the other slash
/// style, and with backslashes doubled as JSON writes them. A Windows-style home (a backslash or a
/// drive letter) is matched without regard to case, as Windows paths are. A home that is empty or
/// only a root (`/`) is left alone: replacing it would damage every path. The longest homes go
/// first, so `/Users/alice` is replaced before a shorter home that is its prefix.
fn scrub(text: &str, homes: &[String]) -> String {
    let mut homes: Vec<&str> = homes
        .iter()
        .map(|h| h.trim().trim_end_matches(['/', '\\']))
        .filter(|h| h.len() > 1)
        .collect();
    homes.sort_by_key(|h| std::cmp::Reverse(h.len()));
    let mut out = text.to_string();
    for home in homes {
        let windows = home.contains('\\') || home.as_bytes().get(1) == Some(&b':');
        let fwd = home.replace('\\', "/");
        let back = home.replace('/', "\\");
        let json = back.replace('\\', "\\\\");
        for form in [json, back, fwd, home.to_string()] {
            out = replace_home(&out, &form, windows);
        }
    }
    out
}

/// Replaces `needle` by `~`, ignoring ASCII case when `ignore_case` is set (ASCII lowercasing
/// keeps byte offsets, so the match positions index the original text).
fn replace_home(text: &str, needle: &str, ignore_case: bool) -> String {
    if needle.is_empty() {
        return text.to_string();
    }
    if !ignore_case {
        return text.replace(needle, "~");
    }
    let (hay, pat) = (text.to_ascii_lowercase(), needle.to_ascii_lowercase());
    let mut out = String::with_capacity(text.len());
    let mut from = 0;
    while let Some(i) = hay[from..].find(&pat) {
        out.push_str(&text[from..from + i]);
        out.push('~');
        from += i + pat.len();
    }
    out.push_str(&text[from..]);
    out
}

/// `text` cut to [`MAX_BYTES`] at a line end, with a marker.
fn bound(mut text: String) -> String {
    const MARK: &str = "[report cut]\n";
    if text.len() <= MAX_BYTES {
        return text;
    }
    let mut cut = MAX_BYTES - MARK.len();
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    let cut = text[..cut].rfind('\n').map_or(0, |i| i + 1);
    text.truncate(cut);
    text.push_str(MARK);
    text
}

/// The newest session folder in `app_dir` that is not `current`.
fn previous_session(app_dir: &Path, current: Option<&Path>) -> Option<PathBuf> {
    let mut found: Vec<(String, PathBuf)> = std::fs::read_dir(app_dir)
        .ok()?
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let path = e.path();
            (is_session_name(&name) && path.is_dir() && Some(path.as_path()) != current)
                .then_some((name, path))
        })
        .collect();
    found.sort();
    found.pop().map(|(_, p)| p)
}

/// The last `n` complete lines of a file, reading at most [`READ_BYTES`] from its end.
fn tail_lines(path: &Path, n: usize) -> Vec<String> {
    let Ok(mut f) = File::open(path) else {
        return Vec::new();
    };
    let len = f.metadata().map_or(0, |m| m.len());
    let start = len.saturating_sub(READ_BYTES);
    if f.seek(SeekFrom::Start(start)).is_err() {
        return Vec::new();
    }
    let mut bytes = Vec::new();
    if f.take(READ_BYTES).read_to_end(&mut bytes).is_err() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&bytes);
    let mut lines: Vec<&str> = text.lines().collect();
    if start > 0 && !lines.is_empty() {
        lines.remove(0); // it started in the middle of a line
    }
    let skip = lines.len().saturating_sub(n);
    lines[skip..].iter().map(|l| l.to_string()).collect()
}

/// The first `n` lines of a file, reading at most [`READ_BYTES`]; `None` when it cannot be read.
fn head_lines(path: &Path, n: usize) -> Option<Vec<String>> {
    let mut bytes = Vec::new();
    File::open(path)
        .ok()?
        .take(READ_BYTES)
        .read_to_end(&mut bytes)
        .ok()?;
    let text = String::from_utf8_lossy(&bytes);
    Some(text.lines().take(n).map(str::to_string).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const SECRET: &str = "sk-abcdefghijklmnopqrstuvwxyz123456";

    fn info<'a>(
        current: Option<&'a Path>,
        app_dir: Option<&'a Path>,
        modules: &'a [String],
    ) -> Diagnostics<'a> {
        Diagnostics {
            app: "crastu-desktop",
            version: "0.1.0",
            os: "macos aarch64",
            modules,
            current,
            app_dir,
            homes: &[],
        }
    }

    fn session(app: &Path, name: &str) -> PathBuf {
        let p = app.join(name);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn it_names_the_version_the_os_and_the_modules() {
        let mods = vec!["dictation".to_string(), "/etc/passwd".to_string()];
        let text = build(&info(None, None, &mods));
        assert!(text.contains("version: 0.1.0"), "{text}");
        assert!(text.contains("os: macos aarch64"), "{text}");
        assert!(text.contains("modules: dictation, invalid-name"), "{text}");
        assert!(text.contains("previous run: none found"), "{text}");
    }

    #[test]
    fn a_missing_or_empty_session_is_said_so() {
        let root = tempfile::tempdir().unwrap();
        let cur = session(root.path(), "20261113T100000Z-1");
        fs::write(cur.join("events.jsonl"), "").unwrap();
        let prev = session(root.path(), "20261112T100000Z-2");
        fs::write(prev.join("panic.txt"), "").unwrap();
        let text = build(&info(Some(&cur), Some(root.path()), &[]));
        assert!(text.contains("last events of this run:\nnone"), "{text}");
        assert!(text.contains("modules: none"), "{text}");
        assert!(text.contains("previous run: 20261112T100000Z-2"), "{text}");
        assert_eq!(text.matches("missing or empty").count(), 3, "{text}");
        // Only the previous run is described, never the current one.
        assert!(!text.contains("20261113T100000Z-1"), "{text}");
    }

    #[test]
    fn it_takes_the_last_events_and_the_headers_of_the_previous_run() {
        let root = tempfile::tempdir().unwrap();
        let cur = session(root.path(), "20261113T100000Z-1");
        let lines: Vec<String> = (0..100).map(|i| format!("{{\"seq\":{i}}}")).collect();
        fs::write(cur.join("events.jsonl"), lines.join("\n") + "\n").unwrap();
        session(root.path(), "20261110T100000Z-9"); // older, must not be the one
        let prev = session(root.path(), "20261112T100000Z-2");
        fs::write(prev.join("end.json"), "{\n  \"status\": \"panicked\"\n}\n").unwrap();
        let panic: String = (0..50).map(|i| format!("panic line {i}\n")).collect();
        fs::write(prev.join("panic.txt"), panic).unwrap();
        let text = build(&info(Some(&cur), Some(root.path()), &[]));
        assert!(text.contains("{\"seq\":99}"), "{text}");
        assert!(text.contains("{\"seq\":60}"), "{text}");
        assert!(!text.contains("{\"seq\":59}"), "{text}");
        assert!(text.contains("\"status\": \"panicked\""), "{text}");
        assert!(text.contains("panic line 0"), "{text}");
        assert!(
            text.contains(&format!("panic line {}", HEADER_LINES - 1)),
            "{text}"
        );
        assert!(
            !text.contains(&format!("panic line {HEADER_LINES}\n")),
            "{text}"
        );
        assert!(text.contains("crash.txt:\n  missing or empty"), "{text}");
    }

    #[test]
    fn no_secret_gets_through_from_any_file() {
        let root = tempfile::tempdir().unwrap();
        let cur = session(root.path(), "20261113T100000Z-1");
        fs::write(
            cur.join("events.jsonl"),
            format!("{{\"msg\":\"key {SECRET}\"}}\n"),
        )
        .unwrap();
        let prev = session(root.path(), "20261112T100000Z-2");
        for f in HEADER_FILES {
            fs::write(
                prev.join(f),
                format!("message: Bearer {SECRET} token=hunter2xyz\n"),
            )
            .unwrap();
        }
        let text = build(&info(Some(&cur), Some(root.path()), &[]));
        assert!(
            !text.contains(SECRET) && !text.contains("hunter2xyz"),
            "{text}"
        );
        assert!(text.contains("[redacted]"), "{text}");
    }

    #[test]
    fn the_report_is_bounded_whatever_the_files_hold() {
        let root = tempfile::tempdir().unwrap();
        let cur = session(root.path(), "20261113T100000Z-1");
        // Many long lines, and a first partial line the tail must drop.
        let line = "x".repeat(5000);
        let big: String = (0..2000).map(|_| format!("{line}\n")).collect();
        fs::write(cur.join("events.jsonl"), &big).unwrap();
        let prev = session(root.path(), "20261112T100000Z-2");
        for f in HEADER_FILES {
            fs::write(prev.join(f), &big).unwrap();
        }
        let text = build(&info(Some(&cur), Some(root.path()), &[]));
        assert!(text.len() <= MAX_BYTES, "{}", text.len());
        assert!(
            text.lines().all(|l| l.chars().count() <= LINE_CHARS + 4),
            "a line was not cut"
        );
    }

    #[test]
    fn a_long_report_is_cut_with_a_marker() {
        let text = bound("a line\n".repeat(MAX_BYTES));
        assert!(text.len() <= MAX_BYTES);
        assert!(text.ends_with("[report cut]\n"));
        assert_eq!(bound("short\n".into()), "short\n");
    }

    #[test]
    fn a_folder_that_is_not_a_session_is_ignored() {
        let root = tempfile::tempdir().unwrap();
        session(root.path(), "notes");
        let text = build(&info(None, Some(root.path()), &[]));
        assert!(text.contains("previous run: none found"), "{text}");
    }

    #[test]
    fn the_home_folder_is_written_as_a_tilde_in_every_line() {
        let root = tempfile::tempdir().unwrap();
        let cur = session(root.path(), "20261113T100000Z-1");
        let prev = session(root.path(), "20261112T100000Z-2");
        fs::write(
            cur.join("events.jsonl"),
            "{\"msg\":\"opened /Users/alice/Music/a.wav\"}\n",
        )
        .unwrap();
        fs::write(
            prev.join("panic.txt"),
            "message: boom\nlocation: /Users/alice/.cargo/registry/src/x/y.rs:1:2\nwin: C:\\Users\\alice\\x.rs\n",
        )
        .unwrap();
        let homes = vec!["/Users/alice".to_string()];
        let mut d = info(Some(&cur), Some(root.path()), &[]);
        d.homes = &homes;
        let text = build(&d);
        assert!(!text.contains("alice"), "{text}");
        assert!(text.contains("~/Music/a.wav"), "{text}");
        assert!(text.contains("~/.cargo/registry"), "{text}");
        // A root or empty home does nothing; so does no home.
        let roots = vec!["/".to_string(), String::new()];
        d.homes = &roots;
        assert!(build(&d).contains("/Users/alice"));
        d.homes = &[];
        assert!(build(&d).contains("/Users/alice"));
    }

    #[test]
    fn windows_homes_match_in_json_escaped_and_other_case_forms() {
        let homes = vec![r"C:\Users\Alice".to_string()];
        for text in [
            r"C:\Users\Alice\x.rs",
            r"c:\users\alice\x.rs",
            r"C:\\Users\\Alice\\x.rs",
            r"c:\\USERS\\ALICE\\x.rs",
            "C:/Users/Alice/x.rs",
            "c:/users/alice/x.rs",
        ] {
            let out = scrub(text, &homes);
            assert!(
                !out.to_ascii_lowercase().contains("alice"),
                "{text} -> {out}"
            );
            assert!(out.starts_with('~'), "{text} -> {out}");
        }
        // The longer home goes first.
        let both = vec!["/Users/al".to_string(), "/Users/alice".to_string()];
        assert_eq!(scrub("/Users/alice/x /Users/al/y", &both), "~/x ~/y");
        // A path at the cut is scrubbed before the cut, so no prefix of the home survives.
        let long = format!("{}/Users/alice/secret", "x".repeat(LINE_CHARS - 6));
        let out = clean(&scrub(&long, &["/Users/alice".to_string()]), LINE_CHARS);
        assert!(!out.contains("/Users/a"), "{out}");
    }

    #[test]
    fn every_home_the_environment_names_is_collected() {
        let env = |k: &str| match k {
            "HOME" => Some("/c/Users/alice".to_string()),
            "USERPROFILE" => Some(r"C:\Users\alice".to_string()),
            "HOMEDRIVE" => Some("C:".to_string()),
            "HOMEPATH" => Some(r"\Users\alice".to_string()),
            _ => None,
        };
        let homes = home_folders(env);
        // USERPROFILE and HOMEDRIVE plus HOMEPATH name the same folder: listed once.
        assert_eq!(homes.len(), 2, "{homes:?}");
        assert!(home_folders(|_| None).is_empty());
        assert!(home_folders(|k| (k == "HOME").then(|| " ".to_string())).is_empty());
    }
}
