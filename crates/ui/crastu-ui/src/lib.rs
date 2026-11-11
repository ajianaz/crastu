//! Design tokens and components for the GPUI front end.
//!
//! GPUI is built on macOS and Windows only for now (Linux comes after Windows), so the crate is
//! empty elsewhere and the Linux CI job needs no X11 or Wayland libraries.
#![cfg(any(target_os = "macos", target_os = "windows"))]

use gpui::App;

/// Call once at start-up, before any window uses a component.
pub fn init(cx: &mut App) {
    gpui_component::init(cx);
}
