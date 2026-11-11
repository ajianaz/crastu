//! Motion: short, ease-out, and only on transform and opacity.

/// The two durations of the design system, in milliseconds.
pub const DURATIONS_MS: [u32; 2] = [120, 200];
pub const SHORT_MS: u32 = DURATIONS_MS[0];
pub const LONG_MS: u32 = DURATIONS_MS[1];

/// The properties that may be animated.
pub const PROPERTIES: [&str; 2] = ["transform", "opacity"];

/// How long an animation lasts: zero when the user asks for reduced motion.
pub fn duration_ms(base_ms: u32, reduce_motion: bool) -> u32 {
    if reduce_motion { 0 } else { base_ms }
}
