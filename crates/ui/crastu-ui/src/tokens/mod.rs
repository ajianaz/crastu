//! Design tokens: the only place where colours, fonts, sizes and durations are written down.
//!
//! Everything here is plain data with no GPUI types, so it builds and is tested on every
//! platform. A component reads its values through [`Tokens`], built from the user's
//! [`Appearance`], and never contains a colour or font literal of its own (`cargo xtask
//! ui-literals` checks that). The values come from `docs/design/tokens.json`; a test compares
//! them all.

pub mod colors;
pub mod layout;
pub mod motion;
pub mod typography;

pub use colors::{Color, Colors, colors};
pub use typography::{Family, FontFamilies, TextStyle, fonts};

use crastu_app::{Appearance, Density};

/// What a component needs to draw itself, for one appearance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub colors: &'static Colors,
    pub fonts: FontFamilies,
    pub density: Density,
    /// Minimum hit target for this density.
    pub hit_target: f32,
    pub reduce_motion: bool,
}

impl Tokens {
    /// `os_reduce_motion` is the system setting, used when the user has not chosen.
    pub fn new(appearance: &Appearance, os_reduce_motion: bool) -> Tokens {
        Tokens {
            colors: colors(appearance.theme),
            fonts: fonts(appearance.font),
            density: appearance.density,
            hit_target: layout::hit_target(appearance.density),
            reduce_motion: appearance.reduce_motion.unwrap_or(os_reduce_motion),
        }
    }

    /// Replaces the families that are not installed with the system fallback.
    pub fn with_installed_fonts(mut self, installed: &[String]) -> Tokens {
        self.fonts = self.fonts.resolve(installed);
        self
    }

    /// Animation duration with the reduce-motion setting applied.
    pub fn duration_ms(&self, base_ms: u32) -> u32 {
        motion::duration_ms(base_ms, self.reduce_motion)
    }
}

#[cfg(test)]
mod tests;
