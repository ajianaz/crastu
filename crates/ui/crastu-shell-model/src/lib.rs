//! What the shell shows, as plain data with no GPUI: the column sizes, the Appearance options and
//! their actions, the command palette, the onboarding steps and the platform trait. `crastu-shell` draws it and
//! re-exports these modules; they build and are tested on every platform.

pub mod about;
pub use crastu_agents_model as agents;
pub use crastu_models_model as models;
pub mod appearance;
pub mod layout;
pub mod palette;
pub mod platform;
#[cfg(test)]
mod settings_tests;
pub mod steps;
