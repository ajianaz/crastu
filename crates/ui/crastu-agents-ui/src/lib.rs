//! The Settings, Agents (MCP) page. The rules and the state are plain data in
//! `crastu-agents-model`; this crate draws them with GPUI and carries out what an intent
//! asks (the clipboard for Copy, the focus for the revoke dialog). The shell owns an
//! [`AgentsPane`], draws [`page`] in the content area and [`dialog`] over the window, and shows
//! the toasts that [`AgentsPane::apply`] returns. It is empty where GPUI is not a dependency
//! (anything but macOS and Windows for now).

#![cfg(any(target_os = "macos", target_os = "windows"))]
#![allow(
    missing_debug_implementations,
    reason = "the pane holds GPUI focus handles"
)]

mod page;
mod pane;

pub use page::{dialog, page};
pub use pane::{AgentSend, AgentsPane, sender};
