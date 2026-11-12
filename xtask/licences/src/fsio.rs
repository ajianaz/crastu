//! Reading licence files and the committed output, and writing the output.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use super::assets::{self, Asset};
use super::render::{Component, Output, Section, SrcFile};

const ASSETS_FILE: &str = "xtask/assets.toml";
const FONTS_DIR: &str = "crates/ui/crastu-ui/assets/fonts";
const FONT_EXTENSIONS: &[&str] = &["ttf", "otf", "ttc", "woff", "woff2"];
/// A licence text above this size is not a licence text; it is an error, not a silent truncation.
pub const MAX_TEXT_BYTES: u64 = 256 * 1024;

/// A text file read as UTF-8, with a size cap. Errors name the file.
pub fn read_text(path: &Path, label: &str) -> Result<String, String> {
    let len = fs::metadata(path)
        .map_err(|e| format!("{label}: {e}"))?
        .len();
    if len > MAX_TEXT_BYTES {
        return Err(format!(
            "{label}: {len} bytes is over the {MAX_TEXT_BYTES} byte limit for a licence text"
        ));
    }
    let bytes = fs::read(path).map_err(|e| format!("{label}: {e}"))?;
    String::from_utf8(bytes).map_err(|_| format!("{label}: not valid UTF-8"))
}

/// The top-level regular files of a package directory that hold a licence or a NOTICE.
/// Symlinks are skipped: a package must not make us read a file outside it.
pub fn licence_files(dir: &Path) -> Result<Vec<SrcFile>, String> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_file = entry.file_type().is_ok_and(|t| t.is_file());
        if !is_licence_file(&name) || !is_file {
            continue;
        }
        let label = format!("{}/{name}", dir.display());
        files.push(SrcFile {
            text: read_text(&entry.path(), &label)?,
            name,
        });
    }
    files.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(files)
}

pub fn is_licence_file(name: &str) -> bool {
    let n = name.to_ascii_uppercase();
    ["LICENSE", "LICENCE", "COPYING", "UNLICENSE", "NOTICE"]
        .iter()
        .any(|p| n.starts_with(p))
}

pub fn read_assets(root: &Path) -> Result<Vec<Asset>, String> {
    let src =
        fs::read_to_string(root.join(ASSETS_FILE)).map_err(|e| format!("{ASSETS_FILE}: {e}"))?;
    let assets = assets::parse(&src)?;
    let found = font_files(root, Path::new(FONTS_DIR))?;
    let problems = assets::coverage(&found, &assets, &|p| root.join(p).is_file());
    if problems.is_empty() {
        Ok(assets)
    } else {
        Err(problems.join("\n"))
    }
}

/// Font files under `dir` (relative to `root`), as `/`-separated repository paths.
fn font_files(root: &Path, dir: &Path) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in fs::read_dir(root.join(&d)).map_err(|e| format!("{}: {e}", d.display()))? {
            let entry = entry.map_err(|e| e.to_string())?;
            let rel = d.join(entry.file_name());
            if entry.path().is_dir() {
                stack.push(rel);
            } else if rel
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| FONT_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
            {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    out.sort();
    Ok(out)
}

pub fn asset_components(root: &Path, assets: &[Asset]) -> Result<Vec<Component>, String> {
    assets
        .iter()
        .map(|a| {
            let text = read_text(&root.join(&a.license_file), &a.license_file)?;
            Ok(Component {
                section: if a.kind == "model" {
                    Section::Model
                } else {
                    Section::Asset
                },
                name: a.name.clone(),
                version: a.version.clone(),
                license: Some(a.spdx.clone()),
                files: vec![SrcFile {
                    name: a
                        .license_file
                        .rsplit('/')
                        .next()
                        .unwrap_or_default()
                        .to_string(),
                    text,
                }],
                forced_id: Some(a.spdx.clone()),
                source: Some(a.source.clone()),
                covers: a.files.clone(),
                note: a.note.clone(),
                authors: Vec::new(),
                repository: None,
            })
        })
        .collect()
}

/// Everything the command owns, as repository path to content.
pub fn expected(out: &Output) -> BTreeMap<String, &str> {
    let mut m: BTreeMap<String, &str> = out
        .files
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str()))
        .collect();
    m.insert("NOTICE".into(), out.notice.as_str());
    m
}

/// What is committed: path to content, plus problems that stop an entry from being compared
/// (a subdirectory, a file that is too large or not UTF-8).
pub fn on_disk(root: &Path) -> (BTreeMap<String, String>, Vec<String>) {
    let mut m = BTreeMap::new();
    let mut problems = Vec::new();
    let notice = root.join("NOTICE");
    if notice.exists() {
        match read_text(&notice, "NOTICE") {
            Ok(t) => {
                m.insert("NOTICE".into(), t);
            }
            Err(e) => problems.push(format!("unreadable: {e}")),
        }
    }
    if let Ok(dir) = fs::read_dir(root.join("LICENSES")) {
        for entry in dir.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = format!("LICENSES/{name}");
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                problems.push(format!(
                    "unexpected directory: {path}; LICENSES/ holds files only, remove it"
                ));
                continue;
            }
            match read_text(&entry.path(), &path) {
                Ok(t) => {
                    m.insert(path, t);
                }
                Err(e) => problems.push(format!("unreadable: {e}")),
            }
        }
    }
    (m, problems)
}

pub fn write(root: &Path, out: &Output) -> Result<(), String> {
    let dir = root.join("LICENSES");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    // Everything in LICENSES/ is generated: remove every entry `--check` would call stale.
    for entry in fs::read_dir(&dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            return Err(format!(
                "LICENSES/{} is a directory; LICENSES/ holds files only, remove it",
                entry.file_name().to_string_lossy()
            ));
        }
        fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
    }
    for (path, text) in expected(out) {
        fs::write(root.join(&path), text).map_err(|e| format!("{path}: {e}"))?;
    }
    println!(
        "licences: wrote NOTICE and {} files in LICENSES/",
        out.files.len()
    );
    Ok(())
}

pub fn verify(root: &Path, out: &Output) -> Result<(), String> {
    let (actual, mut problems) = on_disk(root);
    problems.extend(diff(&expected(out), &actual));
    if problems.is_empty() {
        println!(
            "licences: NOTICE and {} files in LICENSES/ are up to date",
            out.files.len()
        );
        return Ok(());
    }
    for p in &problems {
        println!("{p}");
    }
    Err(format!(
        "{} difference(s) in NOTICE or LICENSES/; run `cargo xtask licences` and commit the result",
        problems.len()
    ))
}

/// A readable summary of how the committed files differ from the expected ones.
pub fn diff(expected: &BTreeMap<String, &str>, actual: &BTreeMap<String, String>) -> Vec<String> {
    let mut out = Vec::new();
    for (path, want) in expected {
        match actual.get(path) {
            None => out.push(format!("missing: {path}")),
            Some(have) if have != want => {
                let line = want
                    .lines()
                    .zip(have.lines())
                    .position(|(a, b)| a != b)
                    .unwrap_or_else(|| want.lines().count().min(have.lines().count()));
                out.push(format!(
                    "changed: {path} (first difference at line {})",
                    line + 1
                ));
            }
            Some(_) => {}
        }
    }
    for path in actual.keys() {
        if !expected.contains_key(path) {
            out.push(format!("stale: {path} is not produced any more"));
        }
    }
    out
}
