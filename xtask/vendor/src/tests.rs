use std::fs;
use std::path::{Path, PathBuf};

use super::compare_trees;

fn tree(name: &str) -> (PathBuf, PathBuf) {
    let base =
        std::env::temp_dir().join(format!("crastu-vendor-test-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let (a, b) = (base.join("a"), base.join("b"));
    for dir in [&a, &b] {
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(dir.join("src/lib.rs"), "fn main() {}\n").unwrap();
        fs::write(dir.join("Cargo.toml"), "[package]\n").unwrap();
    }
    (a, b)
}

fn diff(a: &Path, b: &Path) -> Vec<String> {
    compare_trees(a, b).unwrap()
}

#[test]
fn equal_trees_have_no_difference() {
    let (a, b) = tree("equal");
    assert!(diff(&a, &b).is_empty());
}

#[test]
fn changed_bytes_are_reported() {
    let (a, b) = tree("bytes");
    fs::write(b.join("src/lib.rs"), "fn main() { unsafe {} }\n").unwrap();
    assert_eq!(diff(&a, &b), ["src/lib.rs: content differs"]);
}

#[test]
fn a_missing_and_an_extra_file_are_reported() {
    let (a, b) = tree("files");
    fs::remove_file(b.join("Cargo.toml")).unwrap();
    fs::write(b.join("src/extra.rs"), "").unwrap();
    assert_eq!(
        diff(&a, &b),
        [
            "Cargo.toml: missing from the copy",
            "src/extra.rs: not in the pristine crate plus patch"
        ]
    );
}

#[test]
fn an_extra_empty_directory_is_reported() {
    let (a, b) = tree("emptydir");
    fs::create_dir(b.join("src/hidden")).unwrap();
    assert_eq!(
        diff(&a, &b),
        ["src/hidden: not in the pristine crate plus patch"]
    );
}

#[test]
fn omitted_names_hidden_deeper_in_the_tree_are_found() {
    let (a, b) = tree("hidden");
    fs::create_dir_all(b.join("src/Cargo.lock")).unwrap();
    fs::write(b.join("src/Cargo.lock/x.rs"), "x").unwrap();
    fs::write(b.join("src/LICENSE-APACHE"), "x").unwrap();
    fs::write(b.join("src/.cargo-ok"), "x").unwrap();
    assert_eq!(diff(&a, &b).len(), 4);
}

#[cfg(unix)]
#[test]
fn a_mode_change_is_reported() {
    use std::os::unix::fs::PermissionsExt;
    let (a, b) = tree("mode");
    fs::set_permissions(b.join("src/lib.rs"), fs::Permissions::from_mode(0o755)).unwrap();
    let list = diff(&a, &b);
    assert_eq!(list.len(), 1);
    assert!(
        list[0].starts_with("src/lib.rs: type or permissions differ"),
        "{list:?}"
    );
}

#[cfg(unix)]
#[test]
fn a_symlink_is_not_a_regular_file_with_the_same_content() {
    let (a, b) = tree("symlink");
    fs::write(a.join("real.rs"), "fn main() {}\n").unwrap();
    fs::write(b.join("real.rs"), "fn main() {}\n").unwrap();
    fs::remove_file(b.join("src/lib.rs")).unwrap();
    std::os::unix::fs::symlink("../real.rs", b.join("src/lib.rs")).unwrap();
    let list = diff(&a, &b);
    assert_eq!(list.len(), 1);
    assert!(
        list[0].starts_with("src/lib.rs: type or permissions differ"),
        "{list:?}"
    );
}

#[test]
fn a_clean_patch_report_has_no_inexact_hunk() {
    let report = "patching file src/ycbcr.rs\npatching file src/shaders.metal\n";
    assert!(super::inexact_hunks(report).is_empty());
}

#[test]
fn offsets_fuzz_and_rejects_are_reported() {
    let report = "patching file a.rs\nHunk #1 succeeded at 12 (offset 3 lines).\nHunk #2 succeeded at 40 with fuzz 1.\nHunk #3 FAILED at 90.\n1 out of 3 hunks FAILED -- saving rejects to file a.rs.rej\n";
    assert_eq!(super::inexact_hunks(report).len(), 4);
}
