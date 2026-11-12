//! The pure part of `cargo xtask licences`: from what was read to the files to write.
//!
//! Licence texts are deduplicated by content. `LICENSES/<spdx-id>.txt` holds the first text of an
//! id (in text order), `<spdx-id>-2.txt` the next, and so on. NOTICE maps every component to the
//! files that hold its text. Nothing here reads the disk, the clock or an absolute path.

use std::collections::{BTreeMap, BTreeSet};

use super::expr::parse;
use super::spdx::{bundle_ids, detect_id, file_stem, normalize};
use super::templates::{author_names, generate};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrcFile {
    pub name: String,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Section {
    Asset,
    Model,
    Crate,
}

/// A third-party crate, a bundled asset or a bundled model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Component {
    pub section: Section,
    pub name: String,
    pub version: String,
    /// Declared SPDX expression; `None` when the crate declares none.
    pub license: Option<String>,
    pub files: Vec<SrcFile>,
    /// For assets: the SPDX id of their (single) licence text, known from `assets.toml`.
    pub forced_id: Option<String>,
    pub source: Option<String>,
    /// For assets: the bundled files the licence covers.
    pub covers: Vec<String>,
    /// For assets: a note printed under the entry (for example a Reserved Font Name).
    pub note: Option<String>,
    /// Crate authors and repository from the manifest, named in NOTICE when the package ships no
    /// licence file.
    pub authors: Vec<String>,
    pub repository: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Output {
    pub notice: String,
    /// `LICENSES/<file>` path (with `/`) to its content.
    pub files: BTreeMap<String, String>,
}

struct Resolved<'a> {
    comp: &'a Component,
    /// (kind, text) per licence file: `Some(id)` for a licence, `None` for a NOTICE file.
    texts: Vec<(Option<String>, String)>,
    note: Option<String>,
    /// Declared licences with no text at all in LICENSES/ for this crate.
    uncovered: Vec<String>,
}

pub fn build(components: &[Component]) -> Result<Output, Vec<String>> {
    let mut sorted: Vec<&Component> = components.iter().collect();
    sorted.sort_by(|a, b| (a.section, &a.name, &a.version).cmp(&(b.section, &b.name, &b.version)));

    let mut errors = Vec::new();
    let mut resolved = Vec::new();
    for comp in sorted {
        let expr = comp.license.as_deref().unwrap_or("");
        let mut texts = Vec::new();
        for f in &comp.files {
            if f.text.trim().is_empty() {
                continue; // a blank file is no licence text
            }
            let text = normalize(&f.text);
            if f.name.to_ascii_uppercase().starts_with("NOTICE") {
                texts.push((None, text));
            } else {
                let id = comp
                    .forced_id
                    .clone()
                    .or_else(|| detect_id(&f.name, &text, expr))
                    .unwrap_or_else(|| format!("LicenseRef-{}", comp.name));
                texts.push((Some(id), text));
            }
        }
        let has_licence = texts.iter().any(|(id, _)| id.is_some());
        let mut parts: Vec<String> = Vec::new();
        let mut uncovered = Vec::new();
        match (&comp.license, has_licence) {
            (None, false) => errors.push(format!(
                "{} {} ships no licence file and declares no licence",
                comp.name, comp.version
            )),
            (None, true) => {}
            (Some(declared), _) => {
                // A text of one licence covers it; a bundle of several covers each of them.
                let mut covered: BTreeSet<String> = BTreeSet::new();
                for (id, text) in &texts {
                    match id.as_deref() {
                        Some(id) if id.starts_with("LicenseRef-") => {
                            covered.extend(bundle_ids(text).iter().map(|i| i.to_ascii_lowercase()));
                        }
                        Some(id) => {
                            covered.insert(id.to_ascii_lowercase());
                        }
                        None => {}
                    }
                }
                let expr = parse(declared);
                let missing = expr.missing(&covered);
                let mut generated = Vec::new();
                for id in missing {
                    // Only a package with no licence text at all gets a generated text.
                    let made = if has_licence {
                        None
                    } else {
                        generate(&id, &comp.name, &comp.authors)
                    };
                    if let Some((canon, text)) = made {
                        generated.push(canon.clone());
                        texts.push((Some(canon), text));
                    }
                }
                // What is still missing once the generated texts count (an OR needs only one).
                for g in &generated {
                    covered.insert(g.to_ascii_lowercase());
                }
                uncovered = expr.missing(&covered);
                if !generated.is_empty() {
                    parts.push(format!(
                        "text generated from the declared licence ({}); the crate ships no licence file",
                        generated.join(", ")
                    ));
                }
                if !uncovered.is_empty() {
                    parts.push(format!(
                        "no licence text for {} in the package",
                        uncovered.join(", ")
                    ));
                }
            }
        }
        if !parts.is_empty() {
            let authors = author_names(&comp.authors);
            if !authors.is_empty() {
                parts.push(format!("authors: {}", authors.join(", ")));
            }
            if let Some(repo) = &comp.repository {
                parts.push(format!("repository: {repo}"));
            }
        }
        let note = (!parts.is_empty()).then(|| parts.join("; "));
        resolved.push(Resolved {
            comp,
            texts,
            note,
            uncovered,
        });
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    // Distinct texts per licence id, and distinct NOTICE texts.
    let mut by_id: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut notices: BTreeMap<&str, &str> = BTreeMap::new(); // text -> first component name
    for r in &resolved {
        for (id, text) in &r.texts {
            match id {
                Some(id) => {
                    by_id.entry(id).or_default().insert(text);
                }
                None => {
                    notices.entry(text).or_insert(&r.comp.name);
                }
            }
        }
    }
    let mut files = BTreeMap::new();
    let mut names: BTreeMap<(&str, &str), String> = BTreeMap::new(); // (id or "", text) -> file
    for (id, texts) in &by_id {
        for (i, text) in texts.iter().enumerate() {
            let stem = file_stem(id);
            let base = if i == 0 {
                stem
            } else {
                format!("{stem}-{}", i + 1)
            };
            let name = claim(&files, &base);
            names.insert((id, text), name.clone());
            files.insert(name, (*text).to_string());
        }
    }
    for (text, owner) in &notices {
        let name = claim(&files, &format!("NOTICE-{}", file_stem(owner)));
        names.insert(("", text), name.clone());
        files.insert(name, (*text).to_string());
    }

    let notice = render_notice(&resolved, &names);
    let files = files
        .into_iter()
        .map(|(n, t)| (format!("LICENSES/{n}"), t))
        .collect();
    Ok(Output { notice, files })
}

/// `<base>.txt`, or `<base>-dupN.txt` when another name already taken differs from it only in
/// case or in characters `file_stem` folds together (two ids must never share a file).
fn claim(files: &BTreeMap<String, String>, base: &str) -> String {
    let taken = |name: &str| files.keys().any(|k| k.eq_ignore_ascii_case(name));
    let mut name = format!("{base}.txt");
    let mut n = 2;
    while taken(&name) {
        name = format!("{base}-dup{n}.txt");
        n += 1;
    }
    name
}

fn refs(r: &Resolved, names: &BTreeMap<(&str, &str), String>) -> Vec<String> {
    let set: BTreeSet<String> = r
        .texts
        .iter()
        .filter_map(|(id, text)| names.get(&(id.as_deref().unwrap_or(""), text.as_str())))
        .map(|n| format!("LICENSES/{n}"))
        .collect();
    set.into_iter().collect()
}

fn render_notice(resolved: &[Resolved], names: &BTreeMap<(&str, &str), String>) -> String {
    let mut out = String::from(
        "Crastu\n\
         ======\n\n\
         Crastu is licensed under the Apache License 2.0 (see LICENSE). It includes the\n\
         third-party software and assets listed below. Licence texts are in the LICENSES/\n\
         directory, with the copyright lines of their authors, and each entry names the\n\
         files that hold its text.\n\n\
         Where a crate's package ships no licence file, the entry says so. For MIT, ISC,\n\
         BSD-2-Clause, BSD-3-Clause and Apache-2.0 the text in LICENSES/ is then generated\n\
         from the standard text, with a copyright line built from the authors in the crate's\n\
         manifest, and is marked as generated in its first lines. For any other licence the\n\
         entry carries a note instead of a text; those crates are also listed at the end.\n\n\
         Only what ends up in the shipped binaries is listed: the normal dependencies of\n\
         crastu-desktop and crastu-cli on macOS and Windows. Build scripts, procedural macros\n\
         and test-only crates are not.\n\n\
         This file and LICENSES/ are generated by `cargo xtask licences`; do not edit them\n\
         by hand. CI runs `cargo xtask licences --check`.\n",
    );
    let section = |out: &mut String, title: &str, want: Section, empty: &str| {
        out.push_str(&format!("\n{title}\n{}\n\n", "-".repeat(title.len())));
        let items: Vec<&Resolved> = resolved.iter().filter(|r| r.comp.section == want).collect();
        if items.is_empty() {
            out.push_str(empty);
            out.push('\n');
        }
        if want == Section::Crate && !items.is_empty() {
            out.push_str(&format!("{} third-party crates.\n\n", items.len()));
        }
        for r in items {
            let c = r.comp;
            let expr = c.license.as_deref().unwrap_or("(none declared)");
            if want == Section::Crate {
                let files = refs(r, names).join(", ");
                let tail = match (&r.note, files.is_empty()) {
                    (Some(n), true) => n.clone(),
                    (Some(n), false) => format!("{files}; {n}"),
                    (None, _) => files,
                };
                out.push_str(&format!("- {} {} ({expr}): {tail}\n", c.name, c.version));
            } else {
                out.push_str(&format!("- {} {} ({expr})\n", c.name, c.version));
                if let Some(src) = &c.source {
                    out.push_str(&format!("  Source: {src}\n"));
                }
                for f in &c.covers {
                    out.push_str(&format!("  File: {f}\n"));
                }
                if let Some(note) = &c.note {
                    out.push_str(&format!("  Note: {note}\n"));
                }
                out.push_str(&format!("  Licence text: {}\n", refs(r, names).join(", ")));
            }
        }
    };
    section(&mut out, "Bundled assets", Section::Asset, "None bundled.");
    section(
        &mut out,
        "Models",
        Section::Model,
        "No model weights are bundled. A bundled model is listed here, with its licence id,\n\
         source and licence text, through a [[model]] entry in xtask/assets.toml.",
    );
    section(&mut out, "Rust crates", Section::Crate, "None.");
    let bare: Vec<&Resolved> = resolved
        .iter()
        .filter(|r| !r.uncovered.is_empty())
        .collect();
    if !bare.is_empty() {
        let title = "Crates with no licence text in LICENSES/";
        out.push_str(&format!("\n{title}\n{}\n\n", "-".repeat(title.len())));
        for r in bare {
            let c = r.comp;
            let expr = c.license.as_deref().unwrap_or_default();
            out.push_str(&format!(
                "- {} {} ({expr}): {}\n",
                c.name,
                c.version,
                r.uncovered.join(", ")
            ));
        }
    }
    out
}
