use std::collections::BTreeMap;

use super::assets::{self, Asset};
use super::fsio::{diff, is_licence_file};
use super::render::{Component, Section, SrcFile, build};

pub(super) const MIT_A: &str = "MIT License\n\nCopyright (c) 2020 Alice\n\nPermission is hereby granted, free of charge, to any person obtaining a copy\n";
pub(super) const MIT_B: &str = "MIT License\n\nCopyright (c) 2021 Bob\n\nPermission is hereby granted, free of charge, to any person obtaining a copy\n";
pub(super) const APACHE: &str = "                                 Apache License\n                           Version 2.0, January 2004\n TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION\n";

pub(super) fn krate(
    name: &str,
    version: &str,
    license: Option<&str>,
    files: &[(&str, &str)],
) -> Component {
    Component {
        section: Section::Crate,
        name: name.into(),
        version: version.into(),
        license: license.map(Into::into),
        files: files
            .iter()
            .map(|(n, t)| SrcFile {
                name: (*n).into(),
                text: (*t).into(),
            })
            .collect(),
        forced_id: None,
        source: None,
        covers: Vec::new(),
        note: None,
        authors: vec!["Dana <dana@example.test>".into(), "Eli".into()],
        repository: Some("https://example.test/repo".into()),
    }
}

fn asset(name: &str, files: &[&str], license_file: &str) -> Asset {
    Asset {
        kind: "asset".into(),
        name: name.into(),
        version: "1".into(),
        spdx: "OFL-1.1".into(),
        source: "https://example.test".into(),
        license_file: license_file.into(),
        note: None,
        files: files.iter().map(|f| (*f).into()).collect(),
    }
}

#[test]
fn identical_texts_share_one_file_and_different_texts_are_numbered() {
    let comps = [
        krate("a", "1.0.0", Some("MIT"), &[("LICENSE", MIT_A)]),
        krate(
            "b",
            "2.0.0",
            Some("MIT"),
            &[("LICENSE-MIT", &MIT_A.replace('\n', "\r\n"))],
        ),
        krate("c", "3.0.0", Some("MIT"), &[("LICENSE", MIT_B)]),
    ];
    let out = build(&comps).unwrap();
    let names: Vec<&str> = out.files.keys().map(String::as_str).collect();
    assert_eq!(names, ["LICENSES/MIT-2.txt", "LICENSES/MIT.txt"]);
    // Text order decides the number: Alice (2020) sorts before Bob (2021).
    assert!(out.files["LICENSES/MIT.txt"].contains("Alice"));
    assert!(out.files["LICENSES/MIT-2.txt"].contains("Bob"));
    assert!(out.notice.contains("- a 1.0.0 (MIT): LICENSES/MIT.txt\n"));
    assert!(out.notice.contains("- b 2.0.0 (MIT): LICENSES/MIT.txt\n"));
    assert!(out.notice.contains("- c 3.0.0 (MIT): LICENSES/MIT-2.txt\n"));
}

#[test]
fn a_dual_licensed_crate_maps_to_every_text() {
    let comps = [krate(
        "d",
        "1.0.0",
        Some("MIT OR Apache-2.0"),
        &[("LICENSE-APACHE", APACHE), ("LICENSE-MIT", MIT_A)],
    )];
    let out = build(&comps).unwrap();
    assert!(
        out.notice
            .contains("- d 1.0.0 (MIT OR Apache-2.0): LICENSES/Apache-2.0.txt, LICENSES/MIT.txt\n")
    );
}

#[test]
fn output_does_not_depend_on_input_order() {
    let a = krate("a", "1.0.0", Some("MIT"), &[("LICENSE", MIT_A)]);
    let b = krate("b", "1.0.0", Some("MIT"), &[("LICENSE", MIT_B)]);
    let c = krate("b", "0.9.0", Some("MIT"), &[("LICENSE", MIT_B)]);
    let one = build(&[a.clone(), b.clone(), c.clone()]).unwrap();
    let two = build(&[c, b, a]).unwrap();
    assert_eq!(one, two);
    let notice = &one.notice;
    let pos = |s: &str| notice.find(s).unwrap();
    assert!(pos("- a 1.0.0") < pos("- b 0.9.0") && pos("- b 0.9.0") < pos("- b 1.0.0"));
    assert!(!notice.contains('\r'));
    assert!(notice.ends_with('\n'));
}

#[test]
fn a_crate_with_neither_licence_file_nor_declared_licence_is_an_error() {
    let comps = [krate("f", "1.0.0", None, &[])];
    let err = build(&comps).unwrap_err();
    assert_eq!(err.len(), 1);
    assert!(err[0].contains("f 1.0.0 ships no licence file and declares no licence"));
}

#[test]
fn a_notice_only_package_counts_as_having_no_licence_text() {
    let comps = [krate(
        "g",
        "1.0.0",
        Some("Apache-2.0"),
        &[("NOTICE", "hello")],
    )];
    let out = build(&comps).unwrap();
    assert!(
        out.notice.contains(
            "(Apache-2.0): LICENSES/Apache-2.0.txt, LICENSES/NOTICE-g.txt; text generated"
        )
    );
}

#[test]
fn notice_files_are_kept_next_to_the_licence() {
    let comps = [krate(
        "h",
        "1.0.0",
        Some("Apache-2.0"),
        &[("LICENSE", APACHE), ("NOTICE", "Includes code by Carol")],
    )];
    let out = build(&comps).unwrap();
    assert!(out.files["LICENSES/NOTICE-h.txt"].contains("Carol"));
    assert!(
        out.notice
            .contains("LICENSES/Apache-2.0.txt, LICENSES/NOTICE-h.txt")
    );
}

#[test]
fn unknown_licence_text_gets_a_licenseref_name() {
    let comps = [krate(
        "i",
        "1.0.0",
        Some("MIT OR ISC"),
        &[("LICENSE", "custom words")],
    )];
    let out = build(&comps).unwrap();
    assert!(out.files.contains_key("LICENSES/LicenseRef-i.txt"));
}

#[test]
fn assets_are_rendered_in_their_own_section_with_the_declared_id() {
    let comp = Component {
        section: Section::Asset,
        name: "Inter".into(),
        version: "4.1".into(),
        license: Some("OFL-1.1".into()),
        files: vec![SrcFile {
            name: "OFL.txt".into(),
            text: "Copyright The Inter Project Authors\nno known keywords here\n".into(),
        }],
        forced_id: Some("OFL-1.1".into()),
        source: Some("https://example.test/inter".into()),
        covers: vec!["fonts/Inter-Regular.ttf".into()],
        note: Some("Reserved Font Name: Plex.".into()),
        authors: Vec::new(),
        repository: None,
    };
    let out = build(&[
        comp,
        krate("a", "1.0.0", Some("MIT"), &[("LICENSE", MIT_A)]),
    ])
    .unwrap();
    assert!(out.files["LICENSES/OFL-1.1.txt"].contains("Inter Project"));
    let notice = &out.notice;
    let assets = notice.find("Bundled assets").unwrap();
    let models = notice.find("Models").unwrap();
    let crates = notice.find("Rust crates").unwrap();
    assert!(assets < models && models < crates);
    let inter = notice.find("- Inter 4.1 (OFL-1.1)").unwrap();
    assert!(assets < inter && inter < models);
    assert!(notice.contains("  Source: https://example.test/inter\n"));
    assert!(
        notice.contains("  File: fonts/Inter-Regular.ttf\n  Note: Reserved Font Name: Plex.\n")
    );
    assert!(notice.contains("  Licence text: LICENSES/OFL-1.1.txt\n"));
    assert!(notice.contains("No model weights are bundled"));
}

#[test]
fn assets_file_parses_tables_arrays_and_comments() {
    let src = "# comment\n[[asset]]\nname = \"Inter\"\nversion = \"4.1\"\nspdx = \"OFL-1.1\"\nsource = \"https://x.test/a#b\"\nlicense-file = \"f/OFL.txt\"\nnote = \"Reserved Font Name: X.\"\nfiles = [\n    \"f/a.ttf\",\n    \"f/b.ttf\",\n]\n\n[[model]]\nname = \"m\"\nversion = \"1\"\nspdx = \"Apache-2.0\"\nsource = \"u\"\nlicense-file = \"m/L\"\nfiles = []\n";
    let assets = assets::parse(src).unwrap();
    assert_eq!(assets.len(), 2);
    assert_eq!(assets[0].files, ["f/a.ttf", "f/b.ttf"]);
    assert_eq!(assets[0].source, "https://x.test/a#b");
    assert_eq!(assets[0].note.as_deref(), Some("Reserved Font Name: X."));
    assert_eq!(assets[1].note, None);
    assert_eq!(assets[1].kind, "model");
    assert!(assets[1].files.is_empty());
}

#[test]
fn assets_file_rejects_bad_input() {
    assert!(assets::parse("name = \"x\"\n").is_err());
    assert!(assets::parse("[[font]]\n").is_err());
    assert!(assets::parse("[[asset]]\nname = \"x\"\n").is_err());
    assert!(assets::parse("[[asset]]\nname = x\n").is_err());
    assert!(assets::parse("[[asset]]\nfiles = [\"a\",\n").is_err());
}

#[test]
fn coverage_reports_uncovered_and_missing_files() {
    let a = asset("Inter", &["f/a.ttf", "f/gone.ttf"], "f/OFL.txt");
    let found = vec!["f/a.ttf".to_string(), "f/new.ttf".to_string()];
    let exists = |p: &str| p != "f/gone.ttf";
    let problems = assets::coverage(&found, std::slice::from_ref(&a), &exists);
    assert_eq!(
        problems,
        [
            "Inter lists a file that does not exist: f/gone.ttf",
            "bundled file not covered by xtask/assets.toml: f/new.ttf",
        ]
    );
    let full = assets::coverage(
        &["f/a.ttf".to_string()],
        &[asset("Inter", &["f/a.ttf"], "f/OFL.txt")],
        &|_| true,
    );
    assert!(full.is_empty());
    let no_licence = assets::coverage(&[], &[a], &|p| p != "f/OFL.txt");
    assert!(no_licence.iter().any(|p| p.contains("f/OFL.txt")));
}

#[test]
fn licence_file_names() {
    for n in [
        "LICENSE",
        "LICENSE-MIT",
        "license.txt",
        "COPYING.LIB",
        "UNLICENSE",
        "NOTICE.md",
        "LICENCE",
    ] {
        assert!(is_licence_file(n), "{n}");
    }
    for n in ["Cargo.toml", "README.md", "lib.rs"] {
        assert!(!is_licence_file(n), "{n}");
    }
}

#[test]
fn diff_names_missing_changed_and_stale_files() {
    let expected: BTreeMap<String, &str> = [
        ("NOTICE".to_string(), "a\nb\n"),
        ("LICENSES/MIT.txt".to_string(), "m\n"),
        ("LICENSES/ISC.txt".to_string(), "i\n"),
    ]
    .into();
    let actual: BTreeMap<String, String> = [
        ("NOTICE".to_string(), "a\nX\n".to_string()),
        ("LICENSES/MIT.txt".to_string(), "m\n".to_string()),
        ("LICENSES/OLD.txt".to_string(), "o\n".to_string()),
    ]
    .into();
    assert_eq!(
        diff(&expected, &actual),
        [
            "missing: LICENSES/ISC.txt",
            "changed: NOTICE (first difference at line 2)",
            "stale: LICENSES/OLD.txt is not produced any more",
        ]
    );
    let same: BTreeMap<String, String> = expected
        .iter()
        .map(|(k, v)| (k.clone(), (*v).to_string()))
        .collect();
    assert!(diff(&expected, &same).is_empty());
}

#[test]
fn names_that_fold_to_the_same_file_name_do_not_overwrite_each_other() {
    // Bundles of several licences get a LicenseRef name (a single stub would be no text at all).
    let bundle = |s: &str| format!("{MIT_A}\n---\n{APACHE}\n{s}");
    let comps = [
        krate(
            "foo_bar",
            "1.0.0",
            Some("MIT OR ISC"),
            &[("LICENSE", &bundle("custom one"))],
        ),
        krate(
            "foo-bar",
            "1.0.0",
            Some("MIT OR ISC"),
            &[("LICENSE", &bundle("custom two"))],
        ),
        krate(
            "FOO-BAR",
            "1.0.0",
            Some("MIT OR ISC"),
            &[("LICENSE", &bundle("custom three"))],
        ),
    ];
    let out = build(&comps).unwrap();
    assert_eq!(out.files.len(), 3);
    let texts: Vec<&str> = out.files.values().map(String::as_str).collect();
    for w in ["custom one", "custom two", "custom three"] {
        assert!(texts.iter().any(|t| t.ends_with(&format!("{w}\n"))), "{w}");
    }
    // Every crate points at a file that holds its own text.
    for c in &comps {
        let line = out
            .notice
            .lines()
            .find(|l| l.starts_with(&format!("- {} ", c.name)))
            .unwrap();
        let file = line.split(": ").nth(1).unwrap().split(';').next().unwrap();
        assert_eq!(
            out.files[file],
            xtask_spdx::spdx::normalize(&c.files[0].text)
        );
    }
}

#[test]
fn a_vendored_copy_of_a_third_party_crate_is_listed() {
    use std::path::Path;
    let root = Path::new("/work");
    let own = Path::new("/work/crates/core/x/Cargo.toml");
    let vendored = Path::new("/work/vendor/gpui-pre-apple/Cargo.toml");
    let registry = Path::new("/home/u/.cargo/registry/y/Cargo.toml");
    // Crastu's own crate: no source, outside vendor/.
    assert!(super::is_first_party(true, own, root));
    // A patched copy of a third-party crate: no source, but under vendor/.
    assert!(!super::is_first_party(true, vendored, root));
    // Any registry crate.
    assert!(!super::is_first_party(false, registry, root));
}
