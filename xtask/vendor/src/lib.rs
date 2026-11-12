//! `cargo xtask vendor-check` (crate `xtask-vendor`): the vendored `gpui-pre-apple` must equal the pristine published
//! crate plus `vendor/gpui-pre-apple.patch`, so the patch file is the whole reviewed change.
//!
//! The pristine crate is downloaded (`curl`), checked against the sha256 pinned below
//! (`sha256sum` or `shasum`), unpacked (`tar`), patched (`patch`) and compared by a walker in Rust (path set, file type, executable bit, symlink target, bytes).
//! Those four tools are on the Ubuntu and macOS runners. The files the copy leaves out are
//! listed in [`OMITTED`] and in docs/vendor-gpui.md.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

const URL: &str = "https://static.crates.io/crates/gpui-pre-apple/gpui-pre-apple-0.3.12.crate";
/// sha256 of the published `.crate`; the same value as `checksum` of `gpui-pre-apple` 0.3.12
/// in the registry index.
const SHA256: &str = "a460605ec4e8cbcc9b213462a84b1d6cc3efe4ed8f0e18669659f07b2d5c6aec";
const PATCH: &str = "vendor/gpui-pre-apple.patch";
const VENDORED: &str = "vendor/gpui-pre-apple";
const UNPACKED: &str = "gpui-pre-apple-0.3.12";
/// Files of the published crate that the vendored copy does not carry.
const OMITTED: &[&str] = &[
    ".cargo-ok",
    "Cargo.lock",
    "Cargo.toml.orig",
    "LICENSE-APACHE",
];

pub fn run() -> Result<(), String> {
    let root = std::env::current_dir().map_err(|e| e.to_string())?;
    let patch = root.join(PATCH);
    let vendored = root.join(VENDORED);
    if !patch.is_file() || !vendored.is_dir() {
        return Err(format!(
            "run from the repository root: {PATCH} and {VENDORED} not found"
        ));
    }
    // A fresh private directory (unique name, mode 0700), removed when it goes out of scope.
    let work = tempfile::Builder::new()
        .prefix("crastu-vendor-check-")
        .tempdir()
        .map_err(|e| format!("temporary directory: {e}"))?;
    let result = check(work.path(), &patch, &vendored);
    result?;
    println!("vendor-check: {VENDORED} equals gpui-pre-apple 0.3.12 plus {PATCH}");
    Ok(())
}

fn check(work: &Path, patch: &Path, vendored: &Path) -> Result<(), String> {
    let krate = work.join("pristine.crate");
    tool(
        Command::new("curl")
            .args(["-fsSL", "--retry", "3", "-o"])
            .arg(&krate)
            .arg(URL),
    )?;
    let got = sha256(&krate)?;
    if got != SHA256 {
        return Err(format!(
            "the downloaded crate has sha256 {got}, expected {SHA256}"
        ));
    }
    tool(
        Command::new("tar")
            .arg("-xzf")
            .arg(&krate)
            .arg("-C")
            .arg(work),
    )?;
    let pristine = work.join(UNPACKED);
    // No fuzz, and the output stays visible: the patch must apply exactly, at the line numbers
    // it names. Any offset or fuzz in the report fails the check.
    let out = Command::new("patch")
        .args(["-F0", "-p1", "-i"])
        .arg(patch)
        .current_dir(&pristine)
        .output()
        .map_err(|e| format!("patch: {e}"))?;
    let report = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    if !out.status.success() {
        return Err(format!(
            "{PATCH} does not apply to the pristine crate:\n{report}"
        ));
    }
    let inexact = inexact_hunks(&report);
    if !inexact.is_empty() {
        return Err(format!(
            "{PATCH} does not apply exactly (regenerate it against the pristine crate):\n  {}",
            inexact.join("\n  ")
        ));
    }
    for name in OMITTED {
        // Only the top level of the pristine crate: nested files of that name are compared.
        let path = pristine.join(name);
        if path.is_dir() {
            std::fs::remove_dir_all(&path).map_err(|e| e.to_string())?;
        } else if path.symlink_metadata().is_ok() {
            std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
        if vendored.join(name).symlink_metadata().is_ok() {
            return Err(format!(
                "{VENDORED}/{name} must not exist (omitted on purpose, docs/vendor-gpui.md)"
            ));
        }
    }
    let list = compare_trees(&pristine, vendored)?;
    if list.is_empty() {
        return Ok(());
    }
    Err(format!(
        "{VENDORED} differs from gpui-pre-apple 0.3.12 plus {PATCH} in {} place(s):\n  {}\nany change to the vendored tree needs a regenerated patch (docs/vendor-gpui.md)",
        list.len(),
        list.join("\n  ")
    ))
}

fn tool(cmd: &mut Command) -> Result<(), String> {
    let out = cmd.output().map_err(|e| format!("{cmd:?}: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{cmd:?} failed: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ))
    }
}

fn sha256(file: &Path) -> Result<String, String> {
    for (program, args) in [("sha256sum", &[][..]), ("shasum", &["-a", "256"][..])] {
        if let Ok(out) = Command::new(program).args(args).arg(file).output()
            && out.status.success()
        {
            let text = String::from_utf8_lossy(&out.stdout);
            return Ok(text
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .to_string());
        }
    }
    Err("neither sha256sum nor shasum is available".into())
}

/// The lines of `patch` output that report an offset, fuzz or a rejected hunk.
pub fn inexact_hunks(report: &str) -> Vec<String> {
    report
        .lines()
        .filter(|l| {
            let l = l.to_ascii_lowercase();
            l.contains("offset")
                || l.contains("fuzz")
                || l.contains("failed")
                || l.contains("rejects")
        })
        .map(str::to_string)
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
enum Entry {
    Dir,
    File { exec: bool },
    Symlink(PathBuf),
    Other,
}

fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Entry>) -> Result<(), String> {
    let mut names: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .map(|e| e.map(|e| e.path()))
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    names.sort();
    for path in names {
        let meta = path
            .symlink_metadata()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let rel = path
            .strip_prefix(root)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .into_owned();
        let kind = meta.file_type();
        let entry = if kind.is_symlink() {
            Entry::Symlink(std::fs::read_link(&path).map_err(|e| e.to_string())?)
        } else if kind.is_dir() {
            walk(root, &path, out)?;
            Entry::Dir
        } else if kind.is_file() {
            Entry::File {
                exec: is_exec(&meta),
            }
        } else {
            Entry::Other
        };
        out.insert(rel, entry);
    }
    Ok(())
}

#[cfg(unix)]
fn is_exec(meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_exec(_: &std::fs::Metadata) -> bool {
    false
}

/// Compares two trees by relative path, file type, executable bit, symlink target and bytes,
/// with no name excluded anywhere. One entry per difference, paths relative to the trees.
pub fn compare_trees(want: &Path, have: &Path) -> Result<Vec<String>, String> {
    let (mut a, mut b) = (BTreeMap::new(), BTreeMap::new());
    walk(want, want, &mut a)?;
    walk(have, have, &mut b)?;
    let mut out = Vec::new();
    for (rel, x) in &a {
        match b.get(rel) {
            None => out.push(format!("{rel}: missing from the copy")),
            Some(y) if x != y => out.push(format!(
                "{rel}: type or permissions differ ({x:?} vs {y:?})"
            )),
            Some(_) if matches!(x, Entry::File { .. }) => {
                let (p, q) = (want.join(rel), have.join(rel));
                if std::fs::read(&p).map_err(|e| e.to_string())?
                    != std::fs::read(&q).map_err(|e| e.to_string())?
                {
                    out.push(format!("{rel}: content differs"));
                }
            }
            Some(_) => {}
        }
    }
    for rel in b.keys().filter(|r| !a.contains_key(*r)) {
        out.push(format!("{rel}: not in the pristine crate plus patch"));
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
