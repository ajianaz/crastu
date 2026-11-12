//! What the shell shows, as plain data with no GPUI: the column sizes, the Appearance options and
//! their actions, the command palette and the platform trait. `crastu-shell` draws it and
//! re-exports these modules; they build and are tested on every platform.

pub mod appearance;
pub mod layout;
pub mod palette;
pub mod platform;
