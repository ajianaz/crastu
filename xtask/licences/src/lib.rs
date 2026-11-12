//! `cargo xtask licences`: NOTICE and LICENSES/ for what Crastu ships.
//!
//! ```text
//! cargo xtask licences           write NOTICE and LICENSES/*.txt at the repository root
//! cargo xtask licences --check   fail when the committed files differ from what would be written
//! ```
//!
//! Third-party crates are the normal (not dev, not build) dependency closure of the shipped
//! binaries, `crastu-desktop` and `crastu-cli`, with all features on, for each shipped target,
//! unioned. Each crate's licence texts come from the licence files in its package. Bundled
//! non-Cargo assets (fonts today, models later) come from `xtask/assets.toml`. The output is
//! sorted, has no dates or absolute paths and uses LF line endings, so `--check` is stable.

mod assets;
mod render;
mod spdx;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use assets::Asset;
use render::{Component, Output, Section, SrcFile};

/// Shipped binaries whose dependency closure is listed.
const ROOTS: &[&str] = &["crastu-desktop", "crastu-cli"];
/// Targets Crastu ships (the desktop builds; see `deny.toml` for the wider graph it checks).
const TARGETS: &[&str] = &[
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "x86_64-pc-windows-msvc",
];
const ASSETS_FILE: &str = "xtask/assets.toml";
const FONTS_DIR: &str = "crates/ui/crastu-ui/assets/fonts";
const FONT_EXTENSIONS: &[&str] = &["ttf", "otf", "ttc", "woff", "woff2"];

fn cargo() -> Command {
    Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
}

pub fn run(args: &[String]) -> Result<(), String> {
    let check = match args {
        [] => false,
        [a] if a == "--check" => true,
        _ => return Err("usage: cargo xtask licences [--check]".into()),
    };
    let (root, crates) = shipped_crates()?;
    let assets = read_assets(&root)?;
    let mut components = crates;
    components.extend(asset_components(&root, &assets)?);
    let out = render::build(&components).map_err(|e| e.join("\n"))?;
    if check {
        verify(&root, &out)
    } else {
        write(&root, &out)
    }
}

/// A third-party package of the closure, with what its manifest declares.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Found {
    name: String,
    version: String,
    license: Option<String>,
    authors: Vec<String>,
    repository: Option<String>,
    dir: PathBuf,
}

/// Run `cargo metadata` for one target and return the packages of the normal dependency closure
/// of the shipped binaries as [`Found`].
fn closure(target: &str) -> Result<(PathBuf, BTreeSet<Found>), String> {
    let out = cargo()
        .args(["metadata", "--format-version", "1", "--all-features"])
        .args(["--filter-platform", target])
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    let meta: Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
    let root = PathBuf::from(meta["workspace_root"].as_str().ok_or("no workspace_root")?);
    let packages = meta["packages"].as_array().ok_or("no packages")?;
    let by_id: BTreeMap<&str, &Value> = packages
        .iter()
        .filter_map(|p| Some((p["id"].as_str()?, p)))
        .collect();
    let nodes: BTreeMap<&str, &Value> = meta["resolve"]["nodes"]
        .as_array()
        .ok_or("no resolve graph")?
        .iter()
        .filter_map(|n| Some((n["id"].as_str()?, n)))
        .collect();

    let mut stack: Vec<&str> = packages
        .iter()
        .filter(|p| ROOTS.contains(&p["name"].as_str().unwrap_or_default()))
        .filter_map(|p| p["id"].as_str())
        .collect();
    if stack.len() != ROOTS.len() {
        return Err(format!("expected the packages {ROOTS:?} in the workspace"));
    }
    let mut seen: BTreeSet<&str> = stack.iter().copied().collect();
    while let Some(id) = stack.pop() {
        for dep in nodes[id]["deps"].as_array().into_iter().flatten() {
            let normal = dep["dep_kinds"]
                .as_array()
                .is_some_and(|k| k.iter().any(|k| k["kind"].is_null()));
            let dep_id = dep["pkg"].as_str().unwrap_or_default();
            if normal && seen.insert(dep_id) {
                stack.push(dep_id);
            }
        }
    }
    let mut found = BTreeSet::new();
    for id in seen {
        let p = by_id[id];
        if p["source"].is_null() {
            continue; // a workspace crate: Crastu itself
        }
        let manifest = Path::new(p["manifest_path"].as_str().ok_or("no manifest_path")?);
        let text = |k: &str| p[k].as_str().map(str::to_string);
        found.insert(Found {
            name: text("name").unwrap_or_default(),
            version: text("version").unwrap_or_default(),
            license: text("license"),
            authors: p["authors"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|a| a.as_str().map(str::to_string))
                .collect(),
            repository: text("repository"),
            dir: manifest.parent().unwrap_or(Path::new("")).to_path_buf(),
        });
    }
    Ok((root, found))
}

fn shipped_crates() -> Result<(PathBuf, Vec<Component>), String> {
    let mut root = PathBuf::new();
    let mut all = BTreeSet::new();
    for target in TARGETS {
        let (r, found) = closure(target)?;
        root = r;
        all.extend(found);
    }
    let mut out = Vec::new();
    for f in all {
        out.push(Component {
            section: Section::Crate,
            files: licence_files(&f.dir)?,
            name: f.name,
            version: f.version,
            license: f.license,
            forced_id: None,
            source: None,
            covers: Vec::new(),
            authors: f.authors,
            repository: f.repository,
        });
    }
    Ok((root, out))
}

/// The top-level files of a package directory that hold a licence or a NOTICE.
fn licence_files(dir: &Path) -> Result<Vec<SrcFile>, String> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !is_licence_file(&name) || !entry.path().is_file() {
            continue;
        }
        let bytes = fs::read(entry.path()).map_err(|e| e.to_string())?;
        files.push(SrcFile {
            name,
            text: String::from_utf8_lossy(&bytes).into_owned(),
        });
    }
    files.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(files)
}

fn is_licence_file(name: &str) -> bool {
    let n = name.to_ascii_uppercase();
    ["LICENSE", "LICENCE", "COPYING", "UNLICENSE", "NOTICE"]
        .iter()
        .any(|p| n.starts_with(p))
}

fn read_assets(root: &Path) -> Result<Vec<Asset>, String> {
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

fn asset_components(root: &Path, assets: &[Asset]) -> Result<Vec<Component>, String> {
    assets
        .iter()
        .map(|a| {
            let bytes = fs::read(root.join(&a.license_file))
                .map_err(|e| format!("{}: {e}", a.license_file))?;
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
                    text: String::from_utf8_lossy(&bytes).into_owned(),
                }],
                forced_id: Some(a.spdx.clone()),
                source: Some(a.source.clone()),
                covers: a.files.clone(),
                authors: Vec::new(),
                repository: None,
            })
        })
        .collect()
}

/// Everything the command owns, as repository path to content.
fn expected(out: &Output) -> BTreeMap<String, &str> {
    let mut m: BTreeMap<String, &str> = out
        .files
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str()))
        .collect();
    m.insert("NOTICE".into(), out.notice.as_str());
    m
}

fn on_disk(root: &Path) -> Result<BTreeMap<String, String>, String> {
    let mut m = BTreeMap::new();
    if let Ok(text) = fs::read(root.join("NOTICE")) {
        m.insert("NOTICE".into(), String::from_utf8_lossy(&text).into_owned());
    }
    if let Ok(dir) = fs::read_dir(root.join("LICENSES")) {
        for entry in dir {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let bytes = fs::read(entry.path()).map_err(|e| e.to_string())?;
            m.insert(
                format!("LICENSES/{name}"),
                String::from_utf8_lossy(&bytes).into_owned(),
            );
        }
    }
    Ok(m)
}

fn write(root: &Path, out: &Output) -> Result<(), String> {
    let dir = root.join("LICENSES");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(&dir).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.extension().is_some_and(|e| e == "txt") {
            fs::remove_file(path).map_err(|e| e.to_string())?;
        }
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

fn verify(root: &Path, out: &Output) -> Result<(), String> {
    let problems = diff(&expected(out), &on_disk(root)?);
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
fn diff(expected: &BTreeMap<String, &str>, actual: &BTreeMap<String, String>) -> Vec<String> {
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
