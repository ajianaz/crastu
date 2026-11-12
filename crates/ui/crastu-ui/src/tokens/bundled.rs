//! The fonts that ship with the app (see `assets/fonts/README.md`): Inter and JetBrains Mono,
//! Space Grotesk, IBM Plex Sans and IBM Plex Mono, the three pairings. They are registered with
//! the text system at start-up, so the look does not depend on what is installed.

use std::borrow::Cow;

use gpui::App;

const FILES: [&[u8]; 13] = [
    include_bytes!("../../assets/fonts/inter/Inter-Regular.ttf"),
    include_bytes!("../../assets/fonts/inter/Inter-Medium.ttf"),
    include_bytes!("../../assets/fonts/inter/Inter-SemiBold.ttf"),
    include_bytes!("../../assets/fonts/jetbrains-mono/JetBrainsMono-Regular.ttf"),
    include_bytes!("../../assets/fonts/jetbrains-mono/JetBrainsMono-Medium.ttf"),
    include_bytes!("../../assets/fonts/space-grotesk/SpaceGrotesk-Regular.ttf"),
    include_bytes!("../../assets/fonts/space-grotesk/SpaceGrotesk-Medium.ttf"),
    include_bytes!("../../assets/fonts/space-grotesk/SpaceGrotesk-Bold.ttf"),
    include_bytes!("../../assets/fonts/ibm-plex/IBMPlexSans-Regular.ttf"),
    include_bytes!("../../assets/fonts/ibm-plex/IBMPlexSans-Medium.ttf"),
    include_bytes!("../../assets/fonts/ibm-plex/IBMPlexSans-SemiBold.ttf"),
    include_bytes!("../../assets/fonts/ibm-plex/IBMPlexMono-Regular.ttf"),
    include_bytes!("../../assets/fonts/ibm-plex/IBMPlexMono-Medium.ttf"),
];

/// Registers the bundled fonts. A failure is logged and the text falls back to the system fonts
/// (`FontFamilies::resolve`), so the app still draws.
pub fn load(cx: &App) {
    let fonts = FILES.iter().map(|f| Cow::Borrowed(*f)).collect();
    if let Err(e) = cx.text_system().add_fonts(fonts) {
        eprintln!("crastu: could not load the bundled fonts: {e:#}");
    }
}
