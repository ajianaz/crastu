//! The real binary reads `CRASTU_OFFLINE` from the real environment (passed to a child process,
//! never set in this one). A value that is not valid Unicode must switch Offline on.

#![cfg(unix)]

use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::process::Command;

fn download(offline: Option<OsString>) -> std::process::Output {
    let dir = tempfile::tempdir().unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_crastu-cli"));
    cmd.args(["models", "download", "silero-vad", "--dir"])
        .arg(dir.path().join("models"))
        .env("CRASTU_DATA", dir.path())
        .env_remove("CRASTU_OFFLINE");
    if let Some(v) = offline {
        cmd.env("CRASTU_OFFLINE", v);
    }
    cmd.output().unwrap()
}

#[test]
fn a_non_unicode_offline_variable_refuses_the_download() {
    let out = download(Some(OsString::from_vec(vec![0x66, 0xff, 0xfe])));
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("Offline: model download silero-vad from"),
        "{err}"
    );
}
