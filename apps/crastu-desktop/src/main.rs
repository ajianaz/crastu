//! Crastu desktop app.
//!
//! On macOS and Windows it opens the main window: the `crastu-shell` frame (rail, sidebars,
//! Settings) drawing the `crastu-app` state, with the settings saved in the per-user folder.
//!
//! ```text
//! crastu              open the window
//! crastu --smoke      open it, draw, and quit after 1.5 seconds (for checks)
//! crastu --modules    list which modules are built in, and exit
//! ```

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod window;

use std::process::ExitCode;

fn print_modules() {
    let modules: &[(&str, bool)] = &[
        ("dictation", cfg!(feature = "dictation")),
        ("voice", cfg!(feature = "voice")),
        ("clipper", cfg!(feature = "clipper")),
        ("editor", cfg!(feature = "editor")),
    ];
    for (name, on) in modules {
        println!("{name}: {}", if *on { "built in" } else { "left out" });
    }
}

fn main() -> ExitCode {
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
