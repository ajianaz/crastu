//! `xtask/assets.toml`: bundled files that are not Cargo crates (fonts, later models).
//!
//! Only the small subset of TOML the file uses is read: `[[asset]]` and `[[model]]` tables with
//! string values and arrays of strings, plus `#` comment lines.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    /// `asset` or `model`, from the table header.
    pub kind: String,
    pub name: String,
    pub version: String,
    pub spdx: String,
    pub source: String,
    /// Path of the licence text, relative to the repository root.
    pub license_file: String,
    /// Covered files, relative to the repository root.
    pub files: Vec<String>,
}

type Table = (String, BTreeMap<String, Vec<String>>);

pub fn parse(src: &str) -> Result<Vec<Asset>, String> {
    let mut tables: Vec<Table> = Vec::new();
    let mut pending: Option<(String, String)> = None; // key, collected array text
    for (n, raw) in src.lines().enumerate() {
        let line = raw.trim();
        if let Some((key, mut buf)) = pending.take() {
            buf.push_str(line);
            if line.ends_with(']') {
                insert(&mut tables, key, &buf, n + 1)?;
            } else {
                pending = Some((key, buf));
            }
            continue;
        }
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(head) = line.strip_prefix("[[").and_then(|l| l.strip_suffix("]]")) {
            tables.push((head.trim().to_string(), BTreeMap::new()));
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| format!("assets.toml line {}: expected `key = value`", n + 1))?;
        let (key, value) = (key.trim().to_string(), value.trim());
        if value.starts_with('[') && !value.ends_with(']') {
            pending = Some((key, value.to_string()));
        } else {
            insert(&mut tables, key, value, n + 1)?;
        }
    }
    if pending.is_some() {
        return Err("assets.toml: unterminated array".into());
    }
    tables.into_iter().map(|(kind, t)| build(kind, t)).collect()
}

fn insert(tables: &mut [Table], key: String, value: &str, line: usize) -> Result<(), String> {
    let table = tables
        .last_mut()
        .ok_or_else(|| format!("assets.toml line {line}: value outside a table"))?;
    let values = if let Some(inner) = value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
        inner
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| unquote(s, line))
            .collect::<Result<Vec<_>, _>>()?
    } else {
        vec![unquote(value, line)?]
    };
    table.1.insert(key, values);
    Ok(())
}

fn unquote(s: &str, line: usize) -> Result<String, String> {
    s.strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .map(str::to_string)
        .ok_or_else(|| format!("assets.toml line {line}: expected a quoted string, got `{s}`"))
}

fn build(kind: String, mut t: BTreeMap<String, Vec<String>>) -> Result<Asset, String> {
    if kind != "asset" && kind != "model" {
        return Err(format!("assets.toml: unknown table [[{kind}]]"));
    }
    let mut one = |k: &str| -> Result<String, String> {
        t.remove(k)
            .and_then(|mut v| if v.len() == 1 { v.pop() } else { None })
            .ok_or_else(|| format!("assets.toml: [[{kind}]] needs a string `{k}`"))
    };
    Ok(Asset {
        name: one("name")?,
        version: one("version")?,
        spdx: one("spdx")?,
        source: one("source")?,
        license_file: one("license-file")?,
        files: t.remove("files").unwrap_or_default(),
        kind,
    })
}

/// Problems with how the assets cover the files found on disk: a bundled file nobody lists, and a
/// listed file (or licence file) that does not exist.
pub fn coverage(found: &[String], assets: &[Asset], exists: &dyn Fn(&str) -> bool) -> Vec<String> {
    let mut problems = Vec::new();
    for f in found {
        if !assets.iter().any(|a| a.files.contains(f)) {
            problems.push(format!(
                "bundled file not covered by xtask/assets.toml: {f}"
            ));
        }
    }
    for a in assets {
        for f in a.files.iter().chain(std::iter::once(&a.license_file)) {
            if !exists(f) {
                problems.push(format!("{} lists a file that does not exist: {f}", a.name));
            }
        }
    }
    problems.sort();
    problems
}
