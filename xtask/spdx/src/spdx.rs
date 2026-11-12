//! SPDX helpers: split an expression into licence ids and name a licence text.

/// The licence ids named by an SPDX expression, in order of first appearance.
///
/// Operators (`AND`, `OR`, `WITH`) and parentheses are dropped, and the old Cargo form
/// `MIT/Apache-2.0` is read as `MIT OR Apache-2.0`. A `WITH` exception is dropped with its id.
pub fn spdx_ids(expr: &str) -> Vec<String> {
    let spaced = expr.replace(['(', ')', '/'], " ");
    let mut ids: Vec<String> = Vec::new();
    let mut after_with = false;
    for tok in spaced.split_whitespace() {
        match tok.to_ascii_uppercase().as_str() {
            "AND" | "OR" => after_with = false,
            "WITH" => after_with = true,
            _ if after_with => after_with = false,
            _ => {
                if !ids.iter().any(|i| i == tok) {
                    ids.push(tok.to_string());
                }
            }
        }
    }
    ids
}

/// Best guess of the SPDX id of a licence text: its content first, then its file name or the
/// crate's declared expression when that names a single id and the text carries that licence's
/// terms. `None` when nothing fits (a stub header, a pointer, an unknown text).
pub fn detect_id(file_name: &str, text: &str, expr: &str) -> Option<String> {
    let t = text.to_ascii_lowercase();
    let found = markers(&t);
    match found.as_slice() {
        [id] => return Some((*id).to_string()),
        // One file with several licences (a bundle) is not any one of them.
        [_, _, ..] => return None,
        [] => {}
    }
    // The file name or the declared id may name the licence, but only a text that carries that
    // licence's grant or terms is the licence: a stub header is not.
    let ids = spdx_ids(expr);
    let named = from_file_name(file_name)
        .map(str::to_string)
        .or_else(|| (ids.len() == 1 && !ids[0].starts_with("LicenseRef")).then(|| ids[0].clone()));
    named.filter(|id| carries_terms(id, &t))
}

/// Whether `t` (lowercase) has the operative text of licence `id`, in wording looser than the
/// markers accept. An id this does not know is never accepted by name alone.
fn carries_terms(id: &str, t: &str) -> bool {
    let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    let has = |s: &str| t.contains(s);
    match id.to_ascii_lowercase().as_str() {
        "apache-2.0" => {
            has("terms and conditions for use, reproduction") || has("grant of copyright license")
        }
        "mit" => has("permission is hereby granted"),
        "isc" | "0bsd" => has("permission to use, copy, modify"),
        "bsd-2-clause" | "bsd-3-clause" => has("redistribution and use"),
        "zlib" => has("altered source versions"),
        "mpl-2.0" => has("mozilla public license") && has("1. definitions"),
        "unlicense" => has("unencumbered"),
        _ => false,
    }
}

/// The licences whose own wording a text contains, when it is a bundle of several of them.
pub fn bundle_ids(text: &str) -> Vec<&'static str> {
    let found = markers(&text.to_ascii_lowercase());
    if found.len() >= 2 { found } else { Vec::new() }
}

/// The licences whose own wording the text contains. A short note that only names a licence
/// ("licensed under the Apache License, Version 2.0") is not that licence's text, so Apache
/// needs its terms header and the others their grant sentence.
fn markers(t: &str) -> Vec<&'static str> {
    // Line wrapping must not decide whether a sentence matches.
    let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    let has = |s: &str| t.contains(s);
    let mut out = Vec::new();
    let mut add = |hit: bool, id: &'static str| {
        if hit {
            out.push(id);
        }
    };
    add(
        has("terms and conditions for use, reproduction") && has("apache license"),
        "Apache-2.0",
    );
    add(
        has("mozilla public license") && has("2.0") && has("1. definitions"),
        "MPL-2.0",
    );
    add(has("sil open font license"), "OFL-1.1");
    add(has("boost software license"), "BSL-1.0");
    add(has("this is free and unencumbered software"), "Unlicense");
    add(
        has("cc0") && has("creative commons") && has("statement of purpose"),
        "CC0-1.0",
    );
    add(has("unicode license v3"), "Unicode-3.0");
    // MIT grants to "any person obtaining a copy"; the Boost licence says "any person or
    // organization", the Unicode licence is handled below.
    add(
        has("permission is hereby granted, free of charge, to any person obtaining a copy"),
        "MIT",
    );
    // ISC and 0BSD share their grant sentence; ISC adds the notice condition, 0BSD has none.
    let grant = has("permission to use, copy, modify, and/or distribute")
        || has("permission to use, copy, modify, and distribute");
    let notice_condition = has("provided that the above copyright notice");
    add(grant && notice_condition, "ISC");
    add(grant && !notice_condition, "0BSD");
    let bsd = has("redistribution and use in source and binary forms");
    let three = has("neither the name") || has("endorse or promote");
    add(bsd && three, "BSD-3-Clause");
    add(bsd && !three, "BSD-2-Clause");
    add(has("altered source versions") && has("zlib"), "Zlib");
    // The Unicode licence contains the MIT grant sentence; it is not an MIT text.
    if out.contains(&"Unicode-3.0") {
        out.retain(|id| *id == "Unicode-3.0");
    }
    out
}

fn from_file_name(name: &str) -> Option<&'static str> {
    let n = name.to_ascii_uppercase();
    if n.contains("APACHE") {
        Some("Apache-2.0")
    } else if n.contains("MIT") {
        Some("MIT")
    } else if n.contains("UNLICENSE") {
        Some("Unlicense")
    } else if n.contains("ZLIB") {
        Some("Zlib")
    } else if n.contains("MPL") {
        Some("MPL-2.0")
    } else if n.contains("ISC") {
        Some("ISC")
    } else if n.contains("BSD") {
        Some("BSD-3-Clause")
    } else {
        None
    }
}

/// A licence id made safe for a file name.
pub fn file_stem(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+') {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// Line endings as LF, no trailing blanks on a line, no blank lines at either end, one final LF.
pub fn normalize(text: &str) -> String {
    let unified = text.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = unified.lines().map(str::trim_end).collect();
    let start = lines.iter().position(|l| !l.is_empty()).unwrap_or(0);
    let end = lines
        .iter()
        .rposition(|l| !l.is_empty())
        .map_or(0, |i| i + 1);
    let mut out = lines[start..end.max(start)].join("\n");
    out.push('\n');
    out
}
