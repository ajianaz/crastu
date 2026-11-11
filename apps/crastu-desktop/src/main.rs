//! Crastu desktop app.
//!
//! On macOS and Windows it opens one GPUI window. The window is a placeholder that proves the
//! GPUI stack (workspace pins, licences, CI); `crastu-shell` replaces it (issue #7).
//!
//! ```text
//! crastu              open the window
//! crastu --smoke      open it, draw, and quit after 1.5 seconds (for checks)
//! crastu --modules    list which modules are built in, and exit
//! ```

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod window;

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

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |flag: &str| args.iter().any(|a| a == flag);
    if has("--modules") || !cfg!(any(target_os = "macos", target_os = "windows")) {
        print_modules();
        return;
    }
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    window::open(has("--smoke").then_some(std::time::Duration::from_millis(1500)));
}
