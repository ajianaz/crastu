//! `cargo xtask vendor-check` (crate `xtask-vendor`): the vendored `gpui-pre-apple` must equal the pristine published
//! crate plus `vendor/gpui-pre-apple.patch`, so the patch file is the whole reviewed change.
//!
//! The pristine crate is downloaded (`curl`), checked against the sha256 pinned below
//! (`sha256sum` or `shasum`), unpacked (`tar`), patched (`patch`) and compared (`diff -r`).
//! Those four tools are on the Ubuntu and macOS runners. The files the copy leaves out are
//! listed in [`OMITTED`] and in docs/vendor-gpui.md.

use std::path::Path;
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
    let work = std::env::temp_dir().join(format!("crastu-vendor-check-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let result = check(&work, &patch, &vendored);
    let _ = std::fs::remove_dir_all(&work);
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
    tool(
        Command::new("patch")
            .args(["-s", "-p1", "-i"])
            .arg(patch)
            .current_dir(&pristine),
    )
    .map_err(|e| format!("{PATCH} does not apply to the pristine crate: {e}"))?;
    let mut diff = Command::new("diff");
    diff.arg("-rq");
    for name in OMITTED {
        diff.arg(format!("--exclude={name}"));
    }
    let out = diff
        .arg(&pristine)
        .arg(vendored)
        .output()
        .map_err(|e| format!("diff: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    match out.status.code() {
        Some(0) => Ok(()),
        Some(1) => {
            let list = differences(
                &text,
                &pristine.to_string_lossy(),
                &vendored.to_string_lossy(),
            );
            Err(format!(
                "{VENDORED} differs from gpui-pre-apple 0.3.12 plus {PATCH} in {} file(s):\n  {}\nany change to the vendored tree needs a regenerated patch (docs/vendor-gpui.md)",
                list.len(),
                list.join("\n  ")
            ))
        }
        _ => Err(format!(
            "diff failed: {}",
            String::from_utf8_lossy(&out.stderr)
        )),
    }
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

/// Turns the lines of `diff -rq pristine vendored` into one readable entry per file, paths
/// relative to the copy (`Files A and B differ`, `Only in DIR: NAME`).
pub fn differences(diff_output: &str, pristine: &str, vendored: &str) -> Vec<String> {
    let rel = |full: &str| {
        for base in [pristine, vendored] {
            if let Some(r) = full.strip_prefix(base) {
                return r.trim_start_matches('/').to_string();
            }
        }
        full.to_string()
    };
    let mut out = Vec::new();
    for line in diff_output.lines() {
        if let Some(rest) = line.strip_prefix("Files ")
            && let Some((a, _)) = rest.split_once(" and ")
        {
            out.push(format!("{}: content differs", rel(a)));
        } else if let Some(rest) = line.strip_prefix("Only in ")
            && let Some((dir, name)) = rest.split_once(": ")
        {
            let side = if dir.starts_with(pristine) {
                "missing from the copy"
            } else {
                "not in the pristine crate plus patch"
            };
            out.push(format!("{}: {side}", rel(&format!("{dir}/{name}"))));
        } else if !line.trim().is_empty() {
            out.push(line.to_string());
        }
    }
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::differences;

    const P: &str = "/tmp/w/gpui-pre-apple-0.3.12";
    const V: &str = "/repo/vendor/gpui-pre-apple";

    #[test]
    fn lists_each_kind_of_difference_relative_to_the_copy() {
        let text = format!(
            "Files {P}/src/ycbcr.rs and {V}/src/ycbcr.rs differ\nOnly in {V}/src: extra.rs\nOnly in {P}: build.rs\n"
        );
        assert_eq!(
            differences(&text, P, V),
            [
                "build.rs: missing from the copy",
                "src/extra.rs: not in the pristine crate plus patch",
                "src/ycbcr.rs: content differs",
            ]
        );
    }

    #[test]
    fn nothing_to_list_for_equal_trees() {
        assert!(differences("", P, V).is_empty());
    }

    #[test]
    fn unknown_lines_are_kept() {
        assert_eq!(
            differences("File /a is a regular file while /b is a directory\n", P, V).len(),
            1
        );
    }
}
