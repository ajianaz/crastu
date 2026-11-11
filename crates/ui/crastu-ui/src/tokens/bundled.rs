//! The fonts that ship with the app (see `assets/fonts/README.md`): Inter and JetBrains Mono,
//! the default pairing. They are registered with the text system at start-up, so the default
//! look does not depend on what is installed.

use std::borrow::Cow;

use gpui::App;

const FILES: [&[u8]; 5] = [
    include_bytes!("../../assets/fonts/inter/Inter-Regular.ttf"),
    include_bytes!("../../assets/fonts/inter/Inter-Medium.ttf"),
    include_bytes!("../../assets/fonts/inter/Inter-SemiBold.ttf"),
    include_bytes!("../../assets/fonts/jetbrains-mono/JetBrainsMono-Regular.ttf"),
    include_bytes!("../../assets/fonts/jetbrains-mono/JetBrainsMono-Medium.ttf"),
];

/// Registers the bundled fonts. A failure is logged and the text falls back to the system fonts
/// (`FontFamilies::resolve`), so the app still draws.
pub fn load(cx: &App) {
    let fonts = FILES.iter().map(|f| Cow::Borrowed(*f)).collect();
    if let Err(e) = cx.text_system().add_fonts(fonts) {
        eprintln!("crastu: could not load the bundled fonts: {e:#}");
    }
}
