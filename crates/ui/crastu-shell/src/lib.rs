//! Window, rail, sidebars, command palette and onboarding. Renders crastu-app state.
//!
//! The plain data (`layout`, `appearance`, `palette`, `platform`, in `crastu-shell-model`, and
//! `steps`) builds everywhere; the GPUI `Shell` view (macOS and Windows) draws it and sends
//! `crastu_app::Action`s back.

pub use crastu_shell_model::{agents, appearance, layout, palette, platform};
pub mod steps;

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod view;

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub use view::{NetStart, Shell, init};
