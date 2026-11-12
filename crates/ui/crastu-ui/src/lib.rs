//! Design tokens and components for the GPUI front end.
//!
//! The tokens are plain data and build everywhere. GPUI is a dependency on macOS and Windows
//! only for now (Linux comes after Windows), so the GPUI parts are left out elsewhere and the
//! Linux CI job needs no X11 or Wayland libraries.

pub mod assets;
pub mod tokens;

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub use assets::Assets;

/// Call once at start-up, before any window uses a component. It registers the bundled fonts
/// and initialises the component library.
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub fn init(cx: &mut gpui::App) {
    tokens::bundled::load(cx);
    gpui_component::init(cx);
}
