use std::fs;
use std::path::{Path, PathBuf};

use super::*;

fn lines(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn read_all(dir: &Path) -> String {
    let mut all = String::new();
    for e in fs::read_dir(dir).unwrap().flatten() {
        all.push_str(&fs::read_to_string(e.path()).unwrap_or_default());
    }
    all
}

#[test]
fn the_folder_layout_and_session_json() {
    let root = tempfile::tempdir().unwrap();
    let mut c = Config::new("crastu-cli", "1.2.3");
    c.root_override = Some(root.path().to_path_buf());
    c.modules = vec!["dictation".into(), "voice".into()];
    let rec = Recorder::start_with(c);
    let dir = rec.session_dir().unwrap().to_path_buf();

    let app_dir = root.path().join("crastu-cli");
    assert_eq!(dir.parent().unwrap(), app_dir);
    let name = dir.file_name().unwrap().to_str().unwrap();
    assert!(retention::is_session_name(name), "{name}");
    assert!(
        name.ends_with(&format!("-{}", std::process::id())),
        "{name}"
    );

    let s: Value =
        serde_json::from_str(&fs::read_to_string(dir.join("session.json")).unwrap()).unwrap();
    assert_eq!(s["app"], "crastu-cli");
    assert_eq!(s["version"], "1.2.3");
    assert_eq!(s["os"], std::env::consts::OS);
    assert_eq!(s["modules"], json!(["dictation", "voice"]));
    assert!(s.get("machine_model").is_none());
    assert!(dir.join("events.jsonl").exists());
}

#[test]
fn end_json_only_on_a_clean_finish() {
    let root = tempfile::tempdir().unwrap();
    let rec = Recorder::start("a", "1", Some(root.path()));
    let dir = rec.session_dir().unwrap().to_path_buf();
    assert!(!dir.join("end.json").exists(), "not before finish");
    rec.finish();
    let e: Value =
        serde_json::from_str(&fs::read_to_string(dir.join("end.json")).unwrap()).unwrap();
    assert_eq!(e["clean"], true);
    assert!(e["duration_ms"].is_u64());
    // A second finish does not rewrite it.
    fs::write(dir.join("end.json"), "kept").unwrap();
    rec.finish();
    assert_eq!(fs::read_to_string(dir.join("end.json")).unwrap(), "kept");
}

#[test]
fn dropping_the_last_handle_ends_cleanly_but_a_clone_keeps_it_open() {
    let root = tempfile::tempdir().unwrap();
    let rec = Recorder::start("a", "1", Some(root.path()));
    let dir = rec.session_dir().unwrap().to_path_buf();
    let clone = rec.clone();
    drop(rec);
    assert!(!dir.join("end.json").exists());
    drop(clone);
    assert!(dir.join("end.json").exists());
}

#[test]
fn a_panicking_run_leaves_no_end_json() {
    let root = tempfile::tempdir().unwrap();
    let rec = Recorder::start("a", "1", Some(root.path()));
    let dir = rec.session_dir().unwrap().to_path_buf();
    // The same flag the hook sets; the hook itself is tested in a child process.
    rec.inner.panicked.store(true, Ordering::SeqCst);
    rec.finish();
    drop(rec);
    assert!(!dir.join("end.json").exists());
}

#[test]
fn events_are_flushed_per_line() {
    let root = tempfile::tempdir().unwrap();
    let rec = Recorder::start("a", "1", Some(root.path()));
    let path = rec.session_dir().unwrap().join("events.jsonl");
    rec.info("boot", "first");
    // Read while the recorder is still open and has not finished.
    assert_eq!(lines(&path).len(), 1);
    rec.warn("boot", "second");
    rec.error("boot", "third");
    let l = lines(&path);
    assert_eq!(l.len(), 3);
    assert_eq!(l[0]["level"], "info");
    assert_eq!(l[2]["level"], "error");
    assert_eq!(l[1]["seq"], 2);
    assert_eq!(l[1]["msg"], "second");
    assert!(l[0]["ts"].as_str().unwrap().ends_with('Z'));
}

#[test]
fn perf_lines_are_flushed_and_omit_missing_fields() {
    let root = tempfile::tempdir().unwrap();
    let rec = Recorder::start("a", "1", Some(root.path()));
    let path = rec.session_dir().unwrap().join("perf.jsonl");
    rec.perf(&PerfSample {
        frame_p50_ms: Some(4.0),
        frame_p99_ms: Some(9.5),
        frame_max_ms: Some(20.0),
        rss_bytes: Some(1 << 20),
        ..PerfSample::default()
    });
    let l = lines(&path);
    assert_eq!(l.len(), 1);
    assert_eq!(l[0]["frame_p99_ms"], 9.5);
    assert_eq!(l[0]["rss_bytes"], 1 << 20);
    assert!(l[0].get("queue_depth").is_none());
}

#[test]
fn job_and_mcp_events_carry_only_names_and_enums() {
    let root = tempfile::tempdir().unwrap();
    let rec = Recorder::start("a", "1", Some(root.path()));
    let path = rec.session_dir().unwrap().join("events.jsonl");
    rec.job_event(&JobEvent {
        kind: "transcribe",
        state: JobState::Finished,
        duration_ms: Some(42),
    });
    rec.mcp_event(&McpEvent {
        tool: "project.delete",
        decision: McpDecision::Denied,
    });
    // Free text smuggled into a name field is not recorded.
    rec.mcp_event(&McpEvent {
        tool: "hello, this is my private transcript",
        decision: McpDecision::Allowed,
    });
    rec.job_event(&JobEvent {
        kind: "sk-abcdefghij1234567890",
        state: JobState::Started,
        duration_ms: None,
    });
    let l = lines(&path);
    assert_eq!(l[0]["kind"], "job");
    assert_eq!(l[1]["kind"], "mcp");
    assert_eq!(
        (
            l[0]["job"].clone(),
            l[0]["state"].clone(),
            l[0]["ms"].clone()
        ),
        (json!("transcribe"), json!("finished"), json!(42))
    );
    assert_eq!(
        (l[1]["tool"].clone(), l[1]["decision"].clone()),
        (json!("project.delete"), json!("denied"))
    );
    assert_eq!(l[2]["tool"], "invalid-name");
    assert_eq!(l[3]["job"], "invalid-name");
    let all = read_all(rec.session_dir().unwrap());
    assert!(
        !all.contains("transcript") && !all.contains("sk-abc"),
        "{all}"
    );
}

#[test]
fn a_known_secret_does_not_reach_the_folder() {
    let root = tempfile::tempdir().unwrap();
    let rec = Recorder::start("a", "1", Some(root.path()));
    let key = "sk-ant-api03-SECRETSECRETSECRET123";
    let bearer = "Bearer SECRETBEARERTOKEN9876";
    let agent = "crastu_00112233445566778899aabbccddeeff";
    rec.info("net", &format!("calling with {key}"));
    rec.warn(&format!("t {key}"), &format!("Authorization: {bearer}"));
    rec.error("net", &format!("agent {agent} failed"));
    rec.job_event(&JobEvent {
        kind: key,
        state: JobState::Failed,
        duration_ms: None,
    });
    rec.mcp_event(&McpEvent {
        tool: agent,
        decision: McpDecision::Refused,
    });
    rec.finish();
    let all = read_all(rec.session_dir().unwrap());
    for secret in ["SECRETSECRET", "SECRETBEARER", "00112233445566778899"] {
        assert!(!all.contains(secret), "{secret} leaked: {all}");
    }
    assert!(all.contains("[redacted]"));
    // The surrounding text survives.
    assert!(all.contains("calling with") && all.contains("failed"));
}

#[test]
fn long_messages_are_cut() {
    let root = tempfile::tempdir().unwrap();
    let rec = Recorder::start("a", "1", Some(root.path()));
    rec.info("t", &"x".repeat(5000));
    let l = lines(&rec.session_dir().unwrap().join("events.jsonl"));
    assert!(l[0]["msg"].as_str().unwrap().chars().count() <= 501);
}

#[test]
fn the_root_override_beats_the_environment() {
    let root = tempfile::tempdir().unwrap();
    let env_root = tempfile::tempdir().unwrap();
    let mut c = Config::new("a", "1");
    c.root_override = Some(root.path().to_path_buf());
    let env_path = env_root.path().to_path_buf();
    let rec = Recorder::start_in(c, crastu_settings::Platform::Other, &move |n| {
        (n == LOG_DIR_ENV).then(|| env_path.clone().into_os_string())
    });
    assert!(rec.session_dir().unwrap().starts_with(root.path()));
    assert_eq!(fs::read_dir(env_root.path()).unwrap().count(), 0);
}

#[test]
fn the_environment_moves_the_root_without_an_override() {
    let env_root = tempfile::tempdir().unwrap();
    let env_path = env_root.path().to_path_buf();
    let rec = Recorder::start_in(
        Config::new("a", "1"),
        crastu_settings::Platform::Other,
        &move |n| (n == LOG_DIR_ENV).then(|| env_path.clone().into_os_string()),
    );
    assert!(rec.session_dir().unwrap().starts_with(env_root.path()));
}

#[test]
fn an_unwritable_root_degrades_without_panicking() {
    let dir = tempfile::tempdir().unwrap();
    // A file where the root should be: nothing under it can be created.
    let blocker: PathBuf = dir.path().join("not-a-folder");
    fs::write(&blocker, b"x").unwrap();
    let rec = Recorder::start("a", "1", Some(&blocker));
    assert!(rec.session_dir().is_none());
    rec.info("t", "ignored");
    rec.job_event(&JobEvent {
        kind: "j",
        state: JobState::Started,
        duration_ms: None,
    });
    rec.perf(&PerfSample::default());
    rec.install_panic_hook();
    rec.finish();
    // No home and no override: also inert.
    let rec = Recorder::start_in(
        Config::new("a", "1"),
        crastu_settings::Platform::MacOs,
        &|_| None,
    );
    assert!(rec.session_dir().is_none());
    rec.error("t", "ignored");
}

#[test]
fn a_folder_that_vanishes_mid_run_does_not_panic() {
    let root = tempfile::tempdir().unwrap();
    let rec = Recorder::start("a", "1", Some(root.path()));
    fs::remove_dir_all(root.path()).unwrap();
    rec.info("t", "after the folder is gone");
    rec.finish();
}

#[test]
fn starting_prunes_old_sessions_but_not_the_new_one() {
    let root = tempfile::tempdir().unwrap();
    let app = root.path().join("a");
    for n in 10..15 {
        let d = app.join(format!("202611{n}T000000Z-1"));
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("end.json"), "{}").unwrap();
    }
    let mut c = Config::new("a", "1");
    c.root_override = Some(root.path().to_path_buf());
    c.retention = Retention {
        keep: 3,
        max_bytes: u64::MAX,
    };
    let rec = Recorder::start_with(c);
    let mut names: Vec<_> = fs::read_dir(&app)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert_eq!(names.len(), 3, "{names:?}");
    assert!(rec.session_dir().unwrap().exists());
    // The two newest old ones survive next to the current session.
    assert!(names.contains(&"20261114T000000Z-1".to_string()));
    assert!(names.contains(&"20261113T000000Z-1".to_string()));
}

#[test]
fn two_recorders_in_one_second_get_different_folders() {
    let root = tempfile::tempdir().unwrap();
    let a = Recorder::start("a", "1", Some(root.path()));
    let b = Recorder::start("a", "1", Some(root.path()));
    assert_ne!(a.session_dir(), b.session_dir());
}

#[test]
fn unwinding_past_the_recorder_is_not_a_clean_end() {
    let root = tempfile::tempdir().unwrap();
    let dir = std::sync::Mutex::new(None);
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let rec = Recorder::start("a", "1", Some(root.path()));
        *dir.lock().unwrap() = rec.session_dir().map(Path::to_path_buf);
        panic!("unwinding on purpose");
    }));
    assert!(r.is_err());
    let dir = dir.lock().unwrap().clone().unwrap();
    assert!(!dir.join("end.json").exists());
}
