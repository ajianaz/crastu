//! Spacing, radius, hit targets and the sizes of the shell.

use crastu_app::Density;

/// The spacing scale, in logical pixels.
pub const SPACE: [f32; 7] = [4.0, 8.0, 12.0, 16.0, 20.0, 24.0, 32.0];

pub const RADIUS_CONTROL: f32 = 6.0;
pub const RADIUS_CARD: f32 = 10.0;
pub const RADIUS_MEDIA: f32 = 10.0;
pub const RADIUS_PILL: f32 = 22.0;

/// Minimum hit target of the dictation pill.
pub const HIT_TARGET_PILL: f32 = 44.0;

/// Minimum hit target of a control at the chosen density. The two values live in `crastu-app`
/// with the density setting; a test checks them against the design tokens.
pub fn hit_target(density: Density) -> f32 {
    density.hit_target()
}

pub const SHELL_HEADER: f32 = 44.0;
pub const SHELL_RAIL: f32 = 64.0;
pub const SHELL_LEFT_SIDEBAR: f32 = 236.0;
pub const SHELL_INSPECTOR: f32 = 316.0;
