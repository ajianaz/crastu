//! Crastu desktop app.
//!
//! On macOS and Windows it opens the main window: the `crastu-shell` frame (rail, sidebars,
//! Settings) drawing the `crastu-app` state, with the settings saved in the per-user folder.
//!
//! ```text
//! crastu              open the window
//! crastu mcp          stdio MCP server for AI agents (also: host, stop, version; no window)
//! crastu --smoke      open it, draw, and quit after 1.5 seconds (for checks)
//! crastu --modules    list which modules are built in, and exit
//! ```

#[cfg_attr(
    not(any(target_os = "macos", target_os = "windows")),
    allow(
        dead_code,
        reason = "only the window uses it; the tests run everywhere"
    )
)]
mod agents;
#[cfg_attr(
    not(any(target_os = "macos", target_os = "windows")),
    allow(
        dead_code,
        reason = "only the window uses it; the tests run everywhere"
    )
)]
mod gate;
#[cfg_attr(
    not(any(target_os = "macos", target_os = "windows")),
    allow(
        dead_code,
        reason = "only the window uses it; the tests run everywhere"
    )
)]
mod models;
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod platform;
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod recording;
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod window;

use std::process::ExitCode;

/// Every module and whether it is built in.
const fn modules() -> [(&'static str, bool); 4] {
    [
        ("dictation", cfg!(feature = "dictation")),
        ("voice", cfg!(feature = "voice")),
        ("clipper", cfg!(feature = "clipper")),
        ("editor", cfg!(feature = "editor")),
    ]
}

/// The names of the modules that are built in, for the flight recorder and the diagnostics.
#[cfg_attr(
    not(any(target_os = "macos", target_os = "windows")),
    allow(dead_code, reason = "only the window uses it")
)]
fn built_in_modules() -> Vec<String> {
    modules()
        .iter()
        .filter(|(_, on)| *on)
        .map(|(name, _)| name.to_string())
        .collect()
}

fn print_modules() {
    for (name, on) in modules() {
        println!("{name}: {}", if on { "built in" } else { "left out" });
    }
}

fn main() -> ExitCode {
    // The headless commands (`crastu mcp` and friends) answer before any window is opened.
    let args: Vec<String> = std::env::args().skip(1).collect();
    #[cfg(unix)]
    if args
        .first()
        .is_some_and(|a| crastu_cli::COMMANDS.contains(&a.as_str()))
    {
        return crastu_cli::run("crastu", args);
    }
    #[cfg(not(unix))]
    if args
        .first()
        .is_some_and(|a| ["host", "mcp", "stop", "version", "--version"].contains(&a.as_str()))
    {
        eprintln!(
            "crastu: this command needs the local IPC, which is not available on this platform yet"
        );
        return ExitCode::from(2);
    }
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let has = |flag: &str| args.iter().any(|a| a == flag);
        if !has("--modules") {
            let quit_after = has("--smoke").then_some(std::time::Duration::from_millis(1500));
            window::open(quit_after);
            return ExitCode::SUCCESS;
        }
    }
    print_modules();
    ExitCode::SUCCESS
}
