//! The Settings, Models and licences page. The rules and the state are plain data in
//! `crastu-models-model`; this crate draws them with GPUI and moves the focus when an intent
//! asks. The shell owns a [`ModelsPane`], draws [`page`] in the content area and [`dialog`] over
//! the window, shows the toasts that [`ModelsPane::apply`] and [`ModelsPane::tick`] return, and
//! calls `tick` on a timer while a download runs. It is empty where GPUI is not a dependency
//! (anything but macOS and Windows for now).

#![cfg(any(target_os = "macos", target_os = "windows"))]
#![allow(
    missing_debug_implementations,
    reason = "the pane holds GPUI focus handles"
)]

mod page;
mod pane;

pub use page::{dialog, page};
pub use pane::{ModelsPane, ModelsSend, sender};
