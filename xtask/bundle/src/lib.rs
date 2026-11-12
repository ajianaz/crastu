//! `cargo xtask bundle`: assemble `target/bundle/Crastu.app` from a build of `crastu-desktop`.
//!
//! The pure parts (argument parsing, XML escaping, Info.plist rendering, the file layout) are
//! testable on every platform; the build, copy and `codesign` steps run on macOS only.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

/// Placeholder bundle identifier. Confirm it before signing with a real identity: macOS keys
/// permissions and keychain items on it.
pub const BUNDLE_ID: &str = "dev.crastu.desktop";
/// Minimum macOS version. No better figure was found for GPUI plus Metal, so this is a floor
/// chosen by us; lower it only after testing on an older system.
pub const MIN_MACOS: &str = "13.0";
const MIC_USAGE: &str = "Crastu listens to your microphone while you dictate or record.";
const EXECUTABLE: &str = "crastu";
const APP_NAME: &str = "Crastu";

#[derive(Debug, PartialEq, Eq)]
pub struct Options {
    pub debug: bool,
    /// `None` means ad-hoc (`-`).
    pub identity: Option<String>,
}

pub fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut opts = Options {
        debug: false,
        identity: None,
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--debug" => opts.debug = true,
            "--sign" => {
                let id = it.next().ok_or("--sign needs an identity")?;
                if id.is_empty() || id.starts_with("--") {
                    return Err("--sign needs an identity".into());
                }
                opts.identity = Some(id.clone());
            }
            other => return Err(format!("unknown bundle argument: {other}")),
        }
    }
    Ok(opts)
}

pub fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

fn string(value: &str) -> String {
    format!("<string>{}</string>", xml_escape(value))
}

pub fn info_plist(version: &str) -> String {
    let entries: [(&str, String); 11] = [
        ("CFBundleName", string(APP_NAME)),
        ("CFBundleDisplayName", string(APP_NAME)),
        ("CFBundleExecutable", string(EXECUTABLE)),
        ("CFBundleIdentifier", string(BUNDLE_ID)),
        ("CFBundleVersion", string(version)),
        ("CFBundleShortVersionString", string(version)),
        ("CFBundlePackageType", string("APPL")),
        ("LSMinimumSystemVersion", string(MIN_MACOS)),
        ("NSHighResolutionCapable", "<true/>".into()),
        ("NSPrincipalClass", string("NSApplication")),
        ("NSMicrophoneUsageDescription", string(MIC_USAGE)),
    ];
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <plist version=\"1.0\">\n<dict>\n",
    );
    for (key, value) in entries {
        out.push_str(&format!("\t<key>{key}</key>\n\t{value}\n"));
    }
    out.push_str("</dict>\n</plist>\n");
    out
}

/// Paths inside the bundle, relative to `Crastu.app`, for a set of library file names.
pub fn layout(libs: &[String]) -> Vec<String> {
    let mut paths = vec![
        "Contents/Info.plist".to_string(),
        format!("Contents/MacOS/{EXECUTABLE}"),
    ];
    paths.extend(libs.iter().map(|l| format!("Contents/MacOS/{l}")));
    paths
}

fn dylibs_in(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.extension().is_some_and(|e| e == "dylib") && path.is_file() {
            found.push(path);
        }
    }
    found.sort();
    Ok(found)
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

/// The path of the `crastu` executable in the JSON messages cargo prints with
/// `--message-format=json`: the last `compiler-artifact` of that target that has one.
pub fn executable_from_messages(messages: &str) -> Option<PathBuf> {
    messages
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|m| m["reason"] == "compiler-artifact" && m["target"]["name"] == EXECUTABLE)
        .filter_map(|m| m["executable"].as_str().map(PathBuf::from))
        .next_back()
}

pub fn run_bundle(args: &[String], meta: &Value) -> Result<(), String> {
    if !cfg!(target_os = "macos") {
        return Err("bundle builds a macOS app and only runs on macOS".into());
    }
    let opts = parse_args(args)?;
    let target_dir = PathBuf::from(
        meta["target_directory"]
            .as_str()
            .ok_or("no target_directory in cargo metadata")?,
    );
    let version = meta["packages"]
        .as_array()
        .and_then(|a| a.iter().find(|p| p["name"] == "crastu-desktop"))
        .and_then(|p| p["version"].as_str())
        .ok_or("crastu-desktop version not found")?;

    let mut build = cargo();
    build.args(["build", "-p", "crastu-desktop", "--bin", EXECUTABLE]);
    if !opts.debug {
        build.arg("--release");
    }
    // Cargo says where it put the binary (it differs when CARGO_BUILD_TARGET or `build.target`
    // is set), so ask it instead of guessing `target/{debug,release}`.
    build.arg("--message-format=json-render-diagnostics");
    build.stdout(std::process::Stdio::piped());
    // `output()` would pipe stderr too and hide cargo's progress and errors.
    build.stderr(std::process::Stdio::inherit());
    eprintln!("$ {build:?}");
    let output = build.output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(format!("command failed: {build:?}"));
    }
    let bin = executable_from_messages(&String::from_utf8_lossy(&output.stdout))
        .ok_or("cargo did not report the built binary")?;
    if !bin.is_file() {
        return Err(format!("built binary not found: {}", bin.display()));
    }

    let app = target_dir.join("bundle").join("Crastu.app");
    if app.exists() {
        std::fs::remove_dir_all(&app).map_err(|e| e.to_string())?;
    }
    let macos = app.join("Contents/MacOS");
    std::fs::create_dir_all(&macos).map_err(|e| e.to_string())?;
    std::fs::copy(&bin, macos.join(EXECUTABLE)).map_err(|e| e.to_string())?;
    std::fs::write(app.join("Contents/Info.plist"), info_plist(version))
        .map_err(|e| e.to_string())?;

    let mut lib_names = Vec::new();
    match std::env::var_os("CRASTU_LIBS_DIR") {
        Some(dir) => {
            let libs = dylibs_in(Path::new(&dir))?;
            for lib in &libs {
                let name = lib.file_name().ok_or("library without a name")?;
                // `copy` follows a symlink, so a `libfoo.dylib` that points at `libfoo.1.dylib`
                // is bundled as a real file under each name (the install names keep working).
                std::fs::copy(lib, macos.join(name)).map_err(|e| e.to_string())?;
                lib_names.push(name.to_string_lossy().into_owned());
            }
            println!(
                "bundle: copied {} librar(ies) from CRASTU_LIBS_DIR",
                libs.len()
            );
        }
        None => println!("bundle: CRASTU_LIBS_DIR is not set, no libraries were bundled"),
    }

    for rel in layout(&lib_names) {
        if !app.join(&rel).is_file() {
            return Err(format!("bundle is missing {rel}"));
        }
    }

    // Nested code is signed first, then the app (`--deep` is deprecated for that).
    let identity = opts.identity.as_deref().unwrap_or("-");
    for name in &lib_names {
        let mut sign_lib = Command::new("codesign");
        sign_lib
            .args(["--force", "--sign", identity])
            .arg(macos.join(name));
        run(sign_lib)?;
    }
    let mut sign = Command::new("codesign");
    sign.args(["--force", "--sign", identity]).arg(&app);
    run(sign)?;
    let mut verify = Command::new("codesign");
    verify
        .args(["--verify", "--deep", "--strict", "--verbose=2"])
        .arg(&app);
    run(verify)?;
    println!("bundle: {} signed and verified", app.display());
    Ok(())
}

#[cfg(test)]
mod tests;
