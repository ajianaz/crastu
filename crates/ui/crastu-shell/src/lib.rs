//! Window, rail, sidebars, command palette and onboarding. Renders crastu-app state.
//!
//! `layout` and `appearance` are plain data and build everywhere; the GPUI `Shell` view
//! (macOS and Windows) draws them and sends `crastu_app::Action`s back.

pub mod appearance;
pub mod layout;

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod view;

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub use view::{Shell, init};
