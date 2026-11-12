use std::collections::BTreeSet;

use super::expr::{Expr, parse as try_parse};
use super::spdx::{detect_id, file_stem, normalize, spdx_ids};

const MIT_A: &str = "MIT License\n\nCopyright (c) 2020 Alice\n\nPermission is hereby granted, free of charge, to any person obtaining a copy\n";
const APACHE: &str = "                                 Apache License\n                           Version 2.0, January 2004\n TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION\n";

fn parse(src: &str) -> Expr {
    try_parse(src).unwrap()
}

fn set(ids: &[&str]) -> BTreeSet<String> {
    ids.iter().map(|i| i.to_ascii_lowercase()).collect()
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
    for bad in ["", "  ", "MIT AND", "Zlib ) (", "OR MIT", "MIT WITH", "()"] {
        assert!(try_parse(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn a_with_exception_does_not_turn_an_and_into_an_or() {
    let covered = set(&["MIT"]);
    assert_eq!(
        parse("MIT WITH LLVM-exception AND Zlib").missing(&covered),
        ["Zlib"]
    );
}
