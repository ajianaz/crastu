use std::collections::BTreeMap;

use super::assets::{self, Asset};
use super::render::{Component, Section, SrcFile, build};
use super::spdx::{detect_id, file_stem, normalize, spdx_ids};
use super::{diff, is_licence_file};

const MIT_A: &str = "MIT License\n\nCopyright (c) 2020 Alice\n\nPermission is hereby granted, free of charge, to any person\n";
const MIT_B: &str = "MIT License\n\nCopyright (c) 2021 Bob\n\nPermission is hereby granted, free of charge, to any person\n";
const APACHE: &str = "                                 Apache License\n                           Version 2.0, January 2004\n TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION\n";

fn krate(name: &str, version: &str, license: Option<&str>, files: &[(&str, &str)]) -> Component {
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
        files: files.iter().map(|f| (*f).into()).collect(),
    }
}

#[test]
fn spdx_ids_splits_operators_and_legacy_slash() {
    assert_eq!(spdx_ids("MIT OR Apache-2.0"), ["MIT", "Apache-2.0"]);
    assert_eq!(spdx_ids("MIT/Apache-2.0"), ["MIT", "Apache-2.0"]);
    assert_eq!(
        spdx_ids("(MIT OR Apache-2.0) AND Unicode-3.0"),
        ["MIT", "Apache-2.0", "Unicode-3.0"]
    );
    assert_eq!(spdx_ids("Apache-2.0 WITH LLVM-exception"), ["Apache-2.0"]);
    assert_eq!(spdx_ids("MIT OR MIT"), ["MIT"]);
}

#[test]
fn detect_id_prefers_content_then_name_then_single_expression() {
    // Content wins over a misleading name.
    assert_eq!(
        detect_id("LICENSE-MIT", APACHE, "MIT").as_deref(),
        Some("Apache-2.0")
    );
    assert_eq!(detect_id("LICENSE", MIT_A, "").as_deref(), Some("MIT"));
    assert_eq!(
        detect_id("LICENSE-ZLIB", "x", "MIT").as_deref(),
        Some("Zlib")
    );
    assert_eq!(
        detect_id("LICENSE", "custom words", "ISC").as_deref(),
        Some("ISC")
    );
    assert_eq!(detect_id("LICENSE", "custom words", "MIT OR ISC"), None);
    assert_eq!(detect_id("LICENSE", "custom words", ""), None);
}

#[test]
fn detect_id_does_not_mistake_a_pointer_note_or_a_bundle_for_a_licence() {
    let pointer = "Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>";
    assert_eq!(detect_id("LICENSE", pointer, ""), None);
    assert_eq!(
        detect_id("LICENSE", pointer, "Apache-2.0").as_deref(),
        Some("Apache-2.0")
    );
    let bundle = format!("{MIT_A}\nRedistribution and use in source and binary forms");
    assert_eq!(detect_id("LICENSE-MIT", &bundle, "MIT"), None);
}

#[test]
fn detect_id_tells_bsd_clauses_apart() {
    let two = "Redistribution and use in source and binary forms, with or without\n";
    let three = format!("{two}Neither the name of the copyright holder");
    assert_eq!(detect_id("L", two, "").as_deref(), Some("BSD-2-Clause"));
    assert_eq!(detect_id("L", &three, "").as_deref(), Some("BSD-3-Clause"));
}

#[test]
fn normalize_makes_line_endings_and_edges_stable() {
    assert_eq!(normalize("\r\n\r\na  \r\nb\r\n\r\n\r\n"), "a\nb\n");
    assert_eq!(normalize("a\rb"), "a\nb\n");
    assert_eq!(normalize(""), "\n");
}

#[test]
fn file_stem_keeps_safe_characters_only() {
    assert_eq!(file_stem("LicenseRef-a/b c"), "LicenseRef-a-b-c");
    assert_eq!(file_stem("BSD-3-Clause"), "BSD-3-Clause");
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
fn a_crate_without_a_licence_file_is_named_with_its_authors_and_repository() {
    let comps = [krate("e", "1.0.0", Some("MIT"), &[])];
    let out = build(&comps).unwrap();
    assert!(out.files.is_empty());
    assert!(out.notice.contains(
        "- e 1.0.0 (MIT): no licence file in the package; authors: Dana, Eli; repository: https://example.test/repo\n"
    ));
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
        out.notice
            .contains("(Apache-2.0): LICENSES/NOTICE-g.txt; no licence file in the package")
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
    assert!(notice.contains("  File: fonts/Inter-Regular.ttf\n"));
    assert!(notice.contains("  Licence text: LICENSES/OFL-1.1.txt\n"));
    assert!(notice.contains("No model weights are bundled"));
}

#[test]
fn assets_file_parses_tables_arrays_and_comments() {
    let src = "# comment\n[[asset]]\nname = \"Inter\"\nversion = \"4.1\"\nspdx = \"OFL-1.1\"\nsource = \"https://x.test/a#b\"\nlicense-file = \"f/OFL.txt\"\nfiles = [\n    \"f/a.ttf\",\n    \"f/b.ttf\",\n]\n\n[[model]]\nname = \"m\"\nversion = \"1\"\nspdx = \"Apache-2.0\"\nsource = \"u\"\nlicense-file = \"m/L\"\nfiles = []\n";
    let assets = assets::parse(src).unwrap();
    assert_eq!(assets.len(), 2);
    assert_eq!(assets[0].files, ["f/a.ttf", "f/b.ttf"]);
    assert_eq!(assets[0].source, "https://x.test/a#b");
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
    let comps = [
        krate(
            "foo_bar",
            "1.0.0",
            Some("MIT OR ISC"),
            &[("LICENSE", "custom one")],
        ),
        krate(
            "foo-bar",
            "1.0.0",
            Some("MIT OR ISC"),
            &[("LICENSE", "custom two")],
        ),
        krate(
            "FOO-BAR",
            "1.0.0",
            Some("MIT OR ISC"),
            &[("LICENSE", "custom three")],
        ),
    ];
    let out = build(&comps).unwrap();
    assert_eq!(out.files.len(), 3);
    let texts: Vec<&str> = out.files.values().map(String::as_str).collect();
    for want in ["custom one\n", "custom two\n", "custom three\n"] {
        assert!(texts.contains(&want), "{want}");
    }
    // Every crate points at a file that holds its own text.
    for c in &comps {
        let line = out
            .notice
            .lines()
            .find(|l| l.starts_with(&format!("- {} ", c.name)))
            .unwrap();
        let file = line.split(": ").nth(1).unwrap().split(';').next().unwrap();
        assert_eq!(out.files[file], format!("{}\n", c.files[0].text));
    }
}
