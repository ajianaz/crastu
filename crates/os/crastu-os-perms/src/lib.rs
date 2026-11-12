//! Permission status, requests and System Settings links.
//!
//! Four permissions matter to Crastu on macOS: the microphone, Accessibility (to paste into
//! other apps), Input Monitoring (to see the push-to-talk key) and Screen and System Audio
//! Recording (optional). [`status`] never shows a prompt; [`request`] may; [`open_settings`]
//! opens the right System Settings pane. On other platforms every permission is reported as
//! [`Status::Unknown`] and nothing can be requested (Windows comes in its own phase).
//!
//! The macOS calls are the system frameworks' own functions, so the crate opts in to `unsafe`
//! for them (and only for them); everything else, including the mapping rules, is safe code
//! with tests.

#[cfg(target_os = "macos")]
#[allow(
    unsafe_code,
    reason = "calls into the macOS permission APIs, each one commented"
)]
mod macos;
#[cfg(target_os = "macos")]
use macos as platform;

#[cfg(not(target_os = "macos"))]
mod other;
#[cfg(not(target_os = "macos"))]
use other as platform;

mod rules;

pub use rules::settings_url;

/// The permissions Crastu may ask for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Permission {
    Microphone,
    Accessibility,
    InputMonitoring,
    ScreenAndSystemAudio,
}

impl Permission {
    pub const ALL: [Permission; 4] = [
        Permission::Microphone,
        Permission::Accessibility,
        Permission::InputMonitoring,
        Permission::ScreenAndSystemAudio,
    ];
}

/// What is known about a permission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Granted,
    /// The user refused, or asked and did not grant: the way forward is System Settings.
    Denied,
    /// Not asked yet, or the system does not say (Accessibility, Input Monitoring and Screen
    /// Recording only report granted or not granted until this process has asked).
    Unknown,
}

/// The current status. Never shows a prompt.
pub fn status(p: Permission) -> Status {
    platform::status(p)
}

/// Asks for a permission; the system may show its prompt. `done` gets the status afterwards,
/// possibly from another thread and possibly later (the microphone prompt is asynchronous).
/// Accessibility, Input Monitoring and Screen Recording are granted in System Settings, so
/// `done` may report them as not granted yet: [`status`] shows the change once the user makes it.
pub fn request(p: Permission, done: Box<dyn FnOnce(Status) + Send + 'static>) {
    platform::request(p, done)
}

/// Opens the System Settings pane of a permission.
pub fn open_settings(p: Permission) -> Result<(), String> {
    platform::open_settings(p)
}
