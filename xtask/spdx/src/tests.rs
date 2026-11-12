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

/// The real LICENSE-APACHE of etagere 0.2.15: a copyright and notice header, no terms.
const APACHE_STUB: &str = "Copyright 2020 Nicolas Silva\n\nLicensed under the Apache License, Version 2.0 (the \"License\");\nyou may not use this file except in compliance with the License.\nYou may obtain a copy of the License at\n\n    http://www.apache.org/licenses/LICENSE-2.0\n\nUnless required by applicable law or agreed to in writing, software\ndistributed under the License is distributed on an \"AS IS\" BASIS,\nWITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.\nSee the License for the specific language governing permissions and\nlimitations under the License.\n";

#[test]
fn detect_id_prefers_content_then_name_then_single_expression() {
    // Content wins over a misleading name.
    assert_eq!(
        detect_id("LICENSE-MIT", APACHE, "MIT").as_deref(),
        Some("Apache-2.0")
    );
    assert_eq!(detect_id("LICENSE", MIT_A, "").as_deref(), Some("MIT"));
    // A name or a single declared id decides only for a text that carries that licence's terms.
    let loose_mit = "Permission is hereby granted, free of charge, to any person (the Software)";
    assert_eq!(
        detect_id("LICENSE-MIT", loose_mit, "").as_deref(),
        Some("MIT")
    );
    assert_eq!(
        detect_id("LICENSE", loose_mit, "MIT").as_deref(),
        Some("MIT")
    );
    assert_eq!(detect_id("LICENSE-ZLIB", "x", "MIT"), None);
    assert_eq!(detect_id("LICENSE", "custom words", "ISC"), None);
    assert_eq!(detect_id("LICENSE", "custom words", "MIT OR ISC"), None);
    assert_eq!(detect_id("LICENSE", "custom words", ""), None);
}

#[test]
fn a_stub_header_is_not_the_apache_text_whatever_its_name_or_declared_id() {
    assert_eq!(
        detect_id("LICENSE-APACHE", APACHE_STUB, "MIT/Apache-2.0"),
        None
    );
    assert_eq!(detect_id("LICENSE", APACHE_STUB, "Apache-2.0"), None);
    // With the terms present the name is enough again.
    let with_terms =
        format!("{APACHE_STUB}\nTERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION");
    assert_eq!(
        detect_id("LICENSE-APACHE", &with_terms, "").as_deref(),
        Some("Apache-2.0")
    );
    let grant = format!("{APACHE_STUB}\n2. Grant of Copyright License. Subject to the terms");
    assert_eq!(
        detect_id("LICENSE-APACHE", &grant, "").as_deref(),
        Some("Apache-2.0")
    );
}

#[test]
fn a_short_file_named_after_a_licence_is_not_that_licence_without_its_grant() {
    for (name, id) in [
        ("LICENSE-MIT", "MIT"),
        ("LICENSE-ISC", "ISC"),
        ("LICENSE-ZLIB", "Zlib"),
        ("LICENSE-BSD", "BSD-3-Clause"),
        ("LICENSE-MPL", "MPL-2.0"),
        ("UNLICENSE", "Unlicense"),
        ("LICENSE", "BSL-1.0"),
        ("LICENSE", "Unicode-3.0"),
    ] {
        assert_eq!(
            detect_id(name, "Copyright 2020 Someone", id),
            None,
            "{name}"
        );
        assert_eq!(
            detect_id("LICENSE", "See the repository.", id),
            None,
            "{id}"
        );
    }
}

#[test]
fn detect_id_does_not_mistake_a_pointer_note_or_a_bundle_for_a_licence() {
    let pointer = "Licensed under the Apache License, Version 2.0 <LICENSE-APACHE>";
    assert_eq!(detect_id("LICENSE", pointer, ""), None);
    assert_eq!(detect_id("LICENSE", pointer, "Apache-2.0"), None);
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
