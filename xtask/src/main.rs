//! Repository checks for Crastu.
//!
//! ```text
//! cargo xtask deps-check      layering rules (see rules.rs)
//! cargo xtask bundle          macOS app bundle, ad-hoc signed (see xtask/bundle)
//! cargo xtask build-matrix    build crastu-desktop with each module alone
//! cargo xtask mobile-check    compile portable crates for iOS and Android
//! cargo xtask size-check      lines per file and per crate (see size.rs)
//! cargo xtask ui-literals     no colour or font literal in UI crates (see ui_lint.rs)
//! cargo xtask licences        write NOTICE and LICENSES/ (--check: fail if they are stale)
//! cargo xtask vendor-check    vendor/gpui-pre-apple equals the published crate plus its patch
//! cargo xtask colour-report   BT.709 chart through GPUI's surface() matrix (see xtask/colour)
//! ```

mod rules;
mod size;
mod ui_lint;

use std::process::{Command, ExitCode};

use rules::{Kind, Package};
use serde_json::Value;

const MOBILE_TARGETS: &[&str] = &["aarch64-apple-ios", "aarch64-linux-android"];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("deps-check") => deps_check(),
        Some("build-matrix") => build_matrix(),
        Some("bundle") => metadata().and_then(|meta| xtask_bundle::run_bundle(&args[1..], &meta)),
        Some("mobile-check") => mobile_check(&args[1..]),
        Some("size-check") => metadata().and_then(|meta| size::run(&args[1..], &meta)),
        Some("ui-literals") => metadata().and_then(|meta| ui_lint::run(&meta)),
        Some("licences") => xtask_licences::run(&args[1..]),
        Some("vendor-check") => xtask_vendor::run(),
        Some("colour-report") => colour_report(&args[1..]),
        _ => {
            eprintln!(
                "usage: cargo xtask <deps-check | build-matrix | mobile-check [targets...] | bundle [--debug] [--sign <id>] | size-check [--report | --update] | ui-literals | licences [--check] | colour-report | vendor-check>"
            );
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// `colour-report`: the three pipelines. `--chart`: the chart's NV12 samples, one `y cb cr` line
/// per patch. `--captured <file>`: compare RGB read back from a real render (one `r g b` line
/// per patch, chart order) with the reference.
fn colour_report(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("--chart") => {
            for [y, cb, cr] in xtask_colour::chart_video_ycc() {
                println!("{y} {cb} {cr}");
            }
            return Ok(());
        }
        Some("--captured") => {
            let path = args.get(1).ok_or("--captured needs a file")?;
            let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
            let mut captured = Vec::new();
            for line in text.lines().filter(|l| !l.trim().is_empty()) {
                let v: Vec<u8> = line
                    .split_whitespace()
                    .map(|n| n.parse())
                    .collect::<Result<_, _>>()
                    .map_err(|e| format!("{line}: {e}"))?;
                captured
                    .push(<[u8; 3]>::try_from(v).map_err(|_| format!("{line}: want 3 values"))?);
            }
            let rows = xtask_colour::measure_captured(&captured);
            print!("{}", xtask_colour::markdown(&rows));
            return if xtask_colour::passes(&rows) {
                Ok(())
            } else {
                Err("outside the agreed limits".into())
            };
        }
        _ => {}
    }
    type Pipeline = fn([u8; 3]) -> [u8; 3];
    let pipelines: [(&str, Pipeline); 3] = [
        (
            "GPUI as shipped (BT.601 full range, input expanded by the decoder)",
            xtask_colour::drawn_by_shipped_gpui,
        ),
        (
            "Patched, `420f` tagged BT.709",
            xtask_colour::drawn_by_patched_gpui_420f,
        ),
        (
            "Patched, `420v` tagged BT.709",
            xtask_colour::drawn_by_patched_gpui_420v,
        ),
    ];
    for (title, drawn) in pipelines {
        println!("### {title}\n");
        println!("{}", xtask_colour::markdown(&xtask_colour::measure(drawn)));
    }
    Ok(())
}

fn cargo() -> Command {
    Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
}

fn run(mut cmd: Command) -> Result<(), String> {
    eprintln!("$ {cmd:?}");
    let status = cmd.status().map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("command failed: {cmd:?}"))
    }
}

fn metadata() -> Result<Value, String> {
    let out = cargo()
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())
}

fn crastu_meta(pkg: &Value) -> &Value {
    &pkg["metadata"]["crastu"]
}

fn packages(meta: &Value) -> Vec<Package> {
    let empty = Vec::new();
    meta["packages"]
        .as_array()
        .unwrap_or(&empty)
        .iter()
        .map(|p| {
            let m = crastu_meta(p);
            Package {
                name: p["name"].as_str().unwrap_or_default().to_string(),
                kind: m["kind"].as_str().and_then(Kind::parse),
                group: m["group"].as_str().map(str::to_string),
                composite: m["composite"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default(),
                deps: p["dependencies"]
                    .as_array()
                    .unwrap_or(&empty)
                    .iter()
                    .filter(|d| d["kind"].as_str() != Some("dev"))
                    .filter_map(|d| d["name"].as_str().map(str::to_string))
                    .collect(),
            }
        })
        .collect()
}

fn deps_check() -> Result<(), String> {
    let pkgs = packages(&metadata()?);
    let violations = rules::check(&pkgs);
    if violations.is_empty() {
        println!("deps-check: {} crates, no violations", pkgs.len());
        return Ok(());
    }
    for v in &violations {
        println!("violation: {v}");
    }
    Err(format!("{} dependency rule violation(s)", violations.len()))
}

fn build_matrix() -> Result<(), String> {
    let meta = metadata()?;
    let desktop = meta["packages"]
        .as_array()
        .and_then(|a| a.iter().find(|p| p["name"] == "crastu-desktop"))
        .ok_or("crastu-desktop not found")?;
    let modules: Vec<String> = crastu_meta(desktop)["modules"]
        .as_array()
        .ok_or("crastu-desktop has no [package.metadata.crastu] modules list")?
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();

    let mut none = cargo();
    none.args(["check", "-p", "crastu-desktop", "--no-default-features"]);
    run(none)?;
    for m in &modules {
        let mut c = cargo();
        c.args([
            "check",
            "-p",
            "crastu-desktop",
            "--no-default-features",
            "--features",
            m,
        ]);
        run(c)?;
    }
    println!(
        "build-matrix: no modules plus {} single-module builds passed",
        modules.len()
    );
    Ok(())
}

fn mobile_check(targets: &[String]) -> Result<(), String> {
    let targets: Vec<&str> = if targets.is_empty() {
        MOBILE_TARGETS.to_vec()
    } else {
        targets.iter().map(String::as_str).collect()
    };
    // Default features may pull in C code that needs a cross toolchain (for example bundled
    // SQLite). Mobile builds turn features on explicitly through `mobile-features`.
    let meta = metadata()?;
    let pkgs = packages(&meta);
    let portable: Vec<&str> = pkgs
        .iter()
        .filter(|p| p.kind.is_some_and(Kind::is_portable))
        .map(|p| p.name.as_str())
        .collect();
    let mut features = Vec::new();
    for p in meta["packages"].as_array().into_iter().flatten() {
        let name = p["name"].as_str().unwrap_or_default();
        if !portable.contains(&name) {
            continue;
        }
        for f in crastu_meta(p)["mobile-features"]
            .as_array()
            .into_iter()
            .flatten()
        {
            if let Some(f) = f.as_str() {
                features.push(format!("{name}/{f}"));
            }
        }
    }
    for t in &targets {
        let mut c = cargo();
        c.args(["check", "--target", t, "--no-default-features"]);
        if !features.is_empty() {
            c.args(["--features", &features.join(",")]);
        }
        for p in &portable {
            c.args(["-p", p]);
        }
        run(c)?;
    }
    println!(
        "mobile-check: {} portable crates on {}",
        portable.len(),
        targets.join(", ")
    );
    Ok(())
}
