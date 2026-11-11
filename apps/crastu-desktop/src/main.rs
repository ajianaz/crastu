//! Crastu desktop app. The GPUI window arrives with crastu-shell (issue #7).

fn main() {
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
