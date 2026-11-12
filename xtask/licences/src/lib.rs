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
//!
//! What counts as shipped: `cargo metadata --locked --filter-platform <target>` already drops the
//! dependencies that are not active on that target, so only normal dependencies are followed.
//! Procedural-macro crates (and what only they pull in) run at build time and are not part of the
//! binaries, so they are left out; NOTICE says so. Symlinks in a package are not followed.

mod assets;
mod expr;
mod fsio;
#[cfg(test)]
mod more_tests;
mod render;
mod spdx;
mod templates;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use fsio::{asset_components, licence_files, read_assets, verify, write};
use render::{Component, Section};

/// Shipped binaries whose dependency closure is listed.
const ROOTS: &[&str] = &["crastu-desktop", "crastu-cli"];
/// Targets Crastu ships (the desktop builds; see `deny.toml` for the wider graph it checks).
const TARGETS: &[&str] = &[
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "x86_64-pc-windows-msvc",
];

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

/// Arguments of `cargo metadata` for one target. `--locked`: a Cargo.lock that is out of date
/// must fail here, not be rewritten, or `--check` would pass against a different graph.
fn metadata_args(target: &str) -> [&str; 7] {
    [
        "metadata",
        "--locked",
        "--format-version",
        "1",
        "--all-features",
        "--filter-platform",
        target,
    ]
}

/// Run `cargo metadata` for one target and return the packages of the normal dependency closure
/// of the shipped binaries as [`Found`].
fn closure(target: &str) -> Result<(PathBuf, BTreeSet<Found>), String> {
    let out = cargo()
        .args(metadata_args(target))
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
    let seen = shipped_ids(&meta)?;
    let mut found = BTreeSet::new();
    for id in &seen {
        let p = by_id[id.as_str()];
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

/// Ids of the shipped binaries and everything they reach through normal dependencies, never
/// through a procedural-macro crate. `meta` is `cargo metadata` output filtered to one platform.
fn shipped_ids(meta: &Value) -> Result<BTreeSet<String>, String> {
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
    let no_deps = Vec::new();
    while let Some(id) = stack.pop() {
        let deps = nodes
            .get(id)
            .and_then(|n| n["deps"].as_array())
            .unwrap_or(&no_deps);
        for dep in deps {
            let normal = dep["dep_kinds"]
                .as_array()
                .is_some_and(|k| k.iter().any(|k| k["kind"].is_null()));
            let dep_id = dep["pkg"].as_str().unwrap_or_default();
            let proc_macro = by_id.get(dep_id).is_some_and(|p| is_proc_macro(p));
            if normal && !proc_macro && seen.insert(dep_id) {
                stack.push(dep_id);
            }
        }
    }
    Ok(seen.into_iter().map(str::to_string).collect())
}

fn is_proc_macro(package: &Value) -> bool {
    package["targets"].as_array().is_some_and(|ts| {
        ts.iter().any(|t| {
            t["kind"]
                .as_array()
                .is_some_and(|k| k.iter().any(|k| k == "proc-macro"))
        })
    })
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
            note: None,
            authors: f.authors,
            repository: f.repository,
        });
    }
    Ok((root, out))
}
