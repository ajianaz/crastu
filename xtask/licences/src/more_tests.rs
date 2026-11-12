use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::json;

use super::expr::parse;
use super::fsio::{MAX_TEXT_BYTES, licence_files, on_disk, read_text, verify, write};
use super::render::{Output, build};
use super::spdx::detect_id;
use super::tests::{APACHE, MIT_A, krate};
use super::{is_proc_macro, metadata_args};

fn set(ids: &[&str]) -> BTreeSet<String> {
    ids.iter().map(|i| i.to_ascii_lowercase()).collect()
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("xtask-licences-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn blank_licence_files_are_ignored() {
    let blank = krate("a", "1.0.0", Some("MIT"), &[("LICENSE", " \r\n\t\n")]);
    let out = build(&[blank]).unwrap();
    // Treated as no file: a generated text, never a blank LICENSES file.
    assert!(out.files.values().all(|t| !t.trim().is_empty()));
    assert!(
        out.notice
            .contains("text generated from the declared licence (MIT)")
    );

    let blank_no_licence = krate("b", "1.0.0", None, &[("LICENSE", "\n\n")]);
    let err = build(&[blank_no_licence]).unwrap_err();
    assert!(err[0].contains("b 1.0.0 ships no licence file"));

    let blank_next_to_real = krate(
        "c",
        "1.0.0",
        Some("MIT"),
        &[("LICENSE", ""), ("LICENSE-MIT", MIT_A)],
    );
    let out = build(&[blank_next_to_real]).unwrap();
    assert_eq!(out.files.len(), 1);
    assert!(
        !out.notice
            .contains("text generated from the declared licence")
    );
}

#[test]
fn expressions_are_checked_against_the_texts_found() {
    let covered = set(&["MIT"]);
    assert!(parse("MIT").missing(&covered).is_empty());
    assert_eq!(
        parse("MIT AND Apache-2.0").missing(&covered),
        ["Apache-2.0"]
    );
    assert!(parse("MIT OR Apache-2.0").missing(&covered).is_empty());
    assert_eq!(
        parse("ISC OR Apache-2.0").missing(&covered),
        ["ISC", "Apache-2.0"]
    );
    // AND binds tighter than OR: MIT OR (ISC AND Zlib).
    assert!(parse("MIT OR ISC AND Zlib").missing(&covered).is_empty());
    assert_eq!(parse("(MIT OR ISC) AND Zlib").missing(&covered), ["Zlib"]);
    // Exceptions are dropped, the old slash form is OR, case does not matter.
    assert!(
        parse("mit WITH LLVM-exception")
            .missing(&covered)
            .is_empty()
    );
    assert!(parse("Zlib/MIT").missing(&covered).is_empty());
    // Not parseable: read as an OR of its ids.
    assert!(parse("MIT AND").missing(&covered).is_empty());
    assert_eq!(parse("Zlib ) (").missing(&covered), ["Zlib"]);
}

#[test]
fn an_and_expression_with_one_text_gets_a_note_and_an_or_does_not() {
    let and = krate(
        "a",
        "1.0.0",
        Some("MIT AND Zlib"),
        &[("LICENSE-MIT", MIT_A)],
    );
    let or = krate("b", "1.0.0", Some("MIT OR Zlib"), &[("LICENSE-MIT", MIT_A)]);
    let out = build(&[and, or]).unwrap();
    assert!(out.notice.contains(
        "- a 1.0.0 (MIT AND Zlib): LICENSES/MIT.txt; no licence text for Zlib in the package"
    ));
    assert!(
        out.notice
            .contains("- b 1.0.0 (MIT OR Zlib): LICENSES/MIT.txt\n")
    );
    // The one with a gap is also listed at the end, the other is not.
    let tail = &out.notice[out.notice.find("Crates with no licence text").unwrap()..];
    assert!(tail.contains("- a 1.0.0 (MIT AND Zlib): Zlib\n"));
    assert!(!tail.contains("- b 1.0.0"));
}

#[test]
fn a_crate_without_a_file_gets_a_marked_text_from_a_template() {
    let c = krate("e", "1.0.0", Some("MIT"), &[]);
    let out = build(&[c]).unwrap();
    let text = &out.files["LICENSES/MIT.txt"];
    assert!(text.starts_with(
        "[Generated from the declared licence (MIT); the crate ships no licence file."
    ));
    assert!(text.contains("Copyright (c) Dana, Eli\n"));
    assert!(!text.contains("example.test") && !text.contains('@'));
    assert!(text.contains("Permission is hereby granted"));
    assert!(out.notice.contains(
        "- e 1.0.0 (MIT): LICENSES/MIT.txt; text generated from the declared licence (MIT); the crate ships no licence file; authors: Dana, Eli; repository: https://example.test/repo\n"
    ));
    assert!(!out.notice.contains("Crates with no licence text"));
}

#[test]
fn templates_cover_isc_bsd_and_apache_and_every_operand_of_an_or() {
    let c = krate("f", "1.0.0", Some("MIT OR Apache-2.0"), &[]);
    let isc = krate("g", "1.0.0", Some("ISC"), &[]);
    let bsd2 = krate("h", "1.0.0", Some("BSD-2-Clause"), &[]);
    let bsd3 = krate("i", "1.0.0", Some("BSD-3-Clause"), &[]);
    let out = build(&[c, isc, bsd2, bsd3]).unwrap();
    for f in ["ISC", "BSD-2-Clause", "BSD-3-Clause", "MIT", "Apache-2.0"] {
        assert!(out.files.contains_key(&format!("LICENSES/{f}.txt")), "{f}");
    }
    let apache = &out.files["LICENSES/Apache-2.0.txt"];
    assert!(apache.contains("TERMS AND CONDITIONS FOR USE, REPRODUCTION"));
    assert!(!apache.contains("Dana"));
    assert!(out.files["LICENSES/BSD-3-Clause.txt"].contains("3. Neither the name"));
    assert!(!out.files["LICENSES/BSD-2-Clause.txt"].contains("3. Neither"));
}

#[test]
fn authors_missing_fall_back_to_the_crate_name() {
    let mut c = krate("j", "1.0.0", Some("MIT"), &[]);
    c.authors.clear();
    let out = build(&[c]).unwrap();
    assert!(out.files["LICENSES/MIT.txt"].contains("Copyright (c) the authors of j\n"));
}

#[test]
fn a_licence_without_a_template_stays_a_note_and_is_listed_in_the_header_and_at_the_end() {
    let c = krate("k", "1.0.0", Some("Zlib"), &[]);
    let out = build(&[c]).unwrap();
    assert!(out.files.is_empty());
    assert!(out.notice.contains(
        "- k 1.0.0 (Zlib): no licence text for Zlib in the package; authors: Dana, Eli;"
    ));
    assert!(
        out.notice
            .contains("Crates with no licence text in LICENSES/\n")
    );
    assert!(out.notice.contains("- k 1.0.0 (Zlib): Zlib\n"));
    // The header says what is true about both cases.
    assert!(out.notice.contains("generated\nfrom the standard text"));
    assert!(out.notice.contains("carries a note instead of a text"));
    assert!(out.notice.contains("procedural macros"));
}

#[test]
fn the_unicode_licence_is_not_taken_for_mit() {
    let unicode = "UNICODE LICENSE V3\n\nCOPYRIGHT AND PERMISSION NOTICE\n\nPermission is hereby granted, free of charge, to any person obtaining a copy of data files\n";
    assert_eq!(
        detect_id("LICENSE", unicode, "Unicode-3.0").as_deref(),
        Some("Unicode-3.0")
    );
    let out = build(&[krate(
        "l",
        "1.0.0",
        Some("Unicode-3.0"),
        &[("LICENSE", unicode)],
    )])
    .unwrap();
    assert!(out.files.contains_key("LICENSES/Unicode-3.0.txt"));
    assert!(!out.notice.contains("no licence text"));
    // A plain MIT text still is MIT.
    assert_eq!(detect_id("LICENSE", MIT_A, "").as_deref(), Some("MIT"));
    assert_eq!(
        detect_id("LICENSE", APACHE, "").as_deref(),
        Some("Apache-2.0")
    );
}

#[test]
fn read_text_rejects_big_and_non_utf8_files() {
    let dir = scratch("read");
    let big = dir.join("big");
    fs::write(
        &big,
        vec![b'a'; usize::try_from(MAX_TEXT_BYTES).unwrap() + 1],
    )
    .unwrap();
    assert!(read_text(&big, "big").unwrap_err().contains("over the"));
    let bad = dir.join("bad");
    fs::write(&bad, [0xff, 0xfe, b'a']).unwrap();
    assert!(
        read_text(&bad, "bad")
            .unwrap_err()
            .contains("not valid UTF-8")
    );
    let ok = dir.join("ok");
    fs::write(&ok, "fine\n").unwrap();
    assert_eq!(read_text(&ok, "ok").unwrap(), "fine\n");
    assert!(read_text(&dir.join("none"), "none").is_err());
    fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[test]
fn licence_files_skip_symlinks_and_fail_on_bad_files() {
    let dir = scratch("files");
    fs::write(dir.join("LICENSE-MIT"), MIT_A).unwrap();
    fs::write(dir.join("README.md"), "x").unwrap();
    let outside = scratch("outside");
    fs::write(outside.join("secret"), "secret").unwrap();
    std::os::unix::fs::symlink(outside.join("secret"), dir.join("LICENSE")).unwrap();
    let names: Vec<String> = licence_files(&dir)
        .unwrap()
        .into_iter()
        .map(|f| f.name)
        .collect();
    assert_eq!(names, ["LICENSE-MIT"]);
    fs::write(dir.join("COPYING"), [0xff]).unwrap();
    assert!(licence_files(&dir).unwrap_err().contains("not valid UTF-8"));
    fs::remove_dir_all(dir).unwrap();
    fs::remove_dir_all(outside).unwrap();
}

fn output() -> Output {
    Output {
        notice: "notice\n".into(),
        files: [("LICENSES/MIT.txt".to_string(), "mit\n".to_string())].into(),
    }
}

#[test]
fn write_removes_every_stale_entry_so_check_agrees() {
    let root = scratch("write");
    let lic = root.join("LICENSES");
    fs::create_dir_all(&lic).unwrap();
    for stray in [
        "OLD.txt",
        "UPPER.TXT",
        "README",
        ".hidden",
        "no-extension.md",
    ] {
        fs::write(lic.join(stray), "stray\n").unwrap();
    }
    assert!(verify(&root, &output()).is_err());
    write(&root, &output()).unwrap();
    let names: Vec<String> = fs::read_dir(&lic)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["MIT.txt"]);
    verify(&root, &output()).unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_subdirectory_in_licenses_is_reported_by_check_and_write() {
    let root = scratch("subdir");
    let lic = root.join("LICENSES");
    fs::create_dir_all(lic.join("sub")).unwrap();
    fs::write(lic.join("MIT.txt"), "mit\n").unwrap();
    fs::write(root.join("NOTICE"), "notice\n").unwrap();
    let (files, problems) = on_disk(&root);
    assert_eq!(files.len(), 2);
    assert_eq!(problems.len(), 1);
    assert!(problems[0].contains("unexpected directory: LICENSES/sub"));
    assert!(verify(&root, &output()).is_err());
    assert!(
        write(&root, &output())
            .unwrap_err()
            .contains("LICENSES/sub is a directory")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn proc_macro_packages_are_recognised() {
    let pm = json!({"targets": [{"kind": ["proc-macro"]}]});
    let lib = json!({"targets": [{"kind": ["lib"]}, {"kind": ["bin"]}]});
    assert!(is_proc_macro(&pm));
    assert!(!is_proc_macro(&lib));
    assert!(!is_proc_macro(&json!({})));
}

#[test]
fn a_bundle_text_covers_each_licence_it_contains() {
    let bundle = format!("{MIT_A}\n---\n{APACHE}");
    let c = krate(
        "m",
        "1.0.0",
        Some("MIT AND Apache-2.0"),
        &[("LICENSE", &bundle)],
    );
    let out = build(&[c]).unwrap();
    assert!(out.files.contains_key("LICENSES/LicenseRef-m.txt"));
    assert!(!out.notice.contains("no licence text for"));
}

#[test]
fn generated_texts_count_when_an_or_has_an_operand_without_a_template() {
    let c = krate("n", "1.0.0", Some("Zlib OR MIT"), &[]);
    let out = build(&[c]).unwrap();
    assert!(out.files.contains_key("LICENSES/MIT.txt"));
    assert!(!out.notice.contains("no licence text for"));
    assert!(!out.notice.contains("Crates with no licence text"));
}

#[test]
fn cargo_metadata_runs_locked_and_filtered_by_platform() {
    let args = metadata_args("aarch64-apple-darwin");
    assert!(args.contains(&"--locked"));
    assert!(args.contains(&"--all-features"));
    let at = args.iter().position(|a| *a == "--filter-platform").unwrap();
    assert_eq!(args[at + 1], "aarch64-apple-darwin");
}

fn pkg(name: &str, kind: &str) -> serde_json::Value {
    json!({"id": name, "name": name, "targets": [{"kind": [kind]}]})
}

fn dep(name: &str, kinds: &[Option<&str>]) -> serde_json::Value {
    let kinds: Vec<_> = kinds
        .iter()
        .map(|k| json!({"kind": k, "target": null}))
        .collect();
    json!({"pkg": name, "dep_kinds": kinds})
}

#[test]
fn the_shipped_closure_follows_normal_dependencies_only_and_skips_proc_macros() {
    let meta = json!({
        "packages": [
            pkg("crastu-desktop", "bin"), pkg("crastu-cli", "bin"), pkg("lib", "lib"),
            pkg("macro", "proc-macro"), pkg("macro-dep", "lib"), pkg("dev-only", "lib"),
            pkg("build-only", "lib"), pkg("shared", "lib"), pkg("deep", "lib"),
        ],
        "resolve": {"nodes": [
            {"id": "crastu-desktop", "deps": [
                dep("lib", &[None]), dep("macro", &[None]), dep("dev-only", &[Some("dev")]),
                dep("build-only", &[Some("build")]),
            ]},
            {"id": "crastu-cli", "deps": [dep("shared", &[Some("build"), None])]},
            {"id": "lib", "deps": [dep("deep", &[None]), dep("macro", &[None])]},
            {"id": "macro", "deps": [dep("macro-dep", &[None])]},
            {"id": "shared", "deps": []},
        ]},
    });
    let ids: Vec<String> = super::shipped_ids(&meta).unwrap().into_iter().collect();
    assert_eq!(
        ids,
        ["crastu-cli", "crastu-desktop", "deep", "lib", "shared"]
    );
    // A crate that is also reached normally from a shipped crate stays.
    let mut meta = meta;
    meta["resolve"]["nodes"][2]["deps"] = json!([dep("macro-dep", &[None])]);
    let ids = super::shipped_ids(&meta).unwrap();
    assert!(ids.contains("macro-dep") && !ids.contains("macro"));
    // Both shipped binaries must exist.
    assert!(
        super::shipped_ids(&json!({"packages": [pkg("lib", "lib")], "resolve": {"nodes": []}}))
            .is_err()
    );
}

#[test]
fn a_with_exception_does_not_turn_an_and_into_an_or() {
    let covered = set(&["MIT"]);
    assert_eq!(
        parse("MIT WITH LLVM-exception AND Zlib").missing(&covered),
        ["Zlib"]
    );
}
