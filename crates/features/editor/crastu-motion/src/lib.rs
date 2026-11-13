//! Motion as pure functions of time (part of `crastu-render`, split off for the crate size limit).
//!
//! Every curve is evaluated at an elapsed time in closed form. Nothing is integrated frame by
//! frame, so the value at `t` does not depend on which times were asked for before.

use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

/// Easing curves over normalised time `x` in `0..=1`. Inputs outside it are clamped.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Easing {
    Linear,
    CubicIn,
    CubicOut,
    CubicInOut,
    /// Overshoots the target and settles back (`overshoot` 1.70158 is the usual value).
    BackOut {
        overshoot: f64,
    },
}

impl Easing {
    /// Progress at normalised time `x`: `0.0` at `x <= 0` and `1.0` at `x >= 1` for every curve.
    pub fn at(self, x: f64) -> f64 {
        let x = unit(x);
        match self {
            Self::Linear => x,
            Self::CubicIn => x * x * x,
            Self::CubicOut => {
                let u = 1.0 - x;
                1.0 - u * u * u
            }
            Self::CubicInOut => {
                if x < 0.5 {
                    4.0 * x * x * x
                } else {
                    let u = -2.0 * x + 2.0;
                    1.0 - u * u * u / 2.0
                }
            }
            Self::BackOut { overshoot: c } => {
                // The formula is 0 and 1 at the ends only up to rounding; the ends are exact.
                if x <= 0.0 {
                    return 0.0;
                }
                if x >= 1.0 {
                    return 1.0;
                }
                let u = x - 1.0;
                1.0 + (c + 1.0) * u * u * u + c * u * u
            }
        }
    }
}

/// A damped spring from 0 to 1 with a closed-form solution.
///
/// `duration` is the period of the undamped spring in seconds (as in SwiftUI's
/// `Spring(duration:bounce:)`); `bounce` is `0.0` for critical damping (no overshoot) up to just
/// below `1.0` (very springy). Once the envelope is below about 1e-4 the value is exactly `1.0`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Spring {
    pub duration: f64,
    pub bounce: f64,
}

impl Spring {
    pub const fn new(duration: f64, bounce: f64) -> Self {
        Self { duration, bounce }
    }

    fn params(self) -> (f64, f64) {
        let d = if finite(self.duration) > 1e-6 {
            self.duration
        } else {
            1e-6
        };
        (2.0 * PI / d, 1.0 - finite(self.bounce).clamp(0.0, 0.99))
    }

    /// Seconds after which the spring reads exactly `1.0`.
    pub fn settle_time(self) -> f64 {
        let (omega, zeta) = self.params();
        // The remaining error is the envelope e^(-zeta*omega*t) times a factor of at most about
        // 8 (or 1 + omega*t when critically damped), so this keeps it near 1e-4.
        (8.0e4_f64).ln() / (zeta * omega)
    }

    /// Value at `elapsed` seconds: `0.0` at or before the start.
    pub fn at(self, elapsed: f64) -> f64 {
        let t = nan_to_zero(elapsed);
        if t <= 0.0 {
            return 0.0;
        }
        if t >= self.settle_time() {
            return 1.0;
        }
        let (omega, zeta) = self.params();
        let decay = (-zeta * omega * t).exp();
        if zeta >= 1.0 - 1e-9 {
            1.0 - decay * (1.0 + omega * t)
        } else {
            let wd = omega * (1.0 - zeta * zeta).sqrt();
            1.0 - decay * ((wd * t).cos() + zeta * omega / wd * (wd * t).sin())
        }
    }
}

/// A motion curve of elapsed seconds: an easing over a duration, or a spring.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Curve {
    Ease { easing: Easing, duration: f64 },
    Spring { spring: Spring },
}

impl Curve {
    /// Progress at `elapsed` seconds since the animation started.
    pub fn at(self, elapsed: f64) -> f64 {
        match self {
            Self::Ease { easing, duration } => {
                let d = finite(duration);
                if d <= 0.0 {
                    return if nan_to_zero(elapsed) >= 0.0 {
                        1.0
                    } else {
                        0.0
                    };
                }
                easing.at(nan_to_zero(elapsed) / d)
            }
            Self::Spring { spring } => spring.at(elapsed),
        }
    }
}

/// `NaN` becomes `0.0`; infinities stay, so `+inf` reads as the end of a curve.
fn nan_to_zero(v: f64) -> f64 {
    if v.is_nan() { 0.0 } else { v }
}

fn unit(v: f64) -> f64 {
    nan_to_zero(v).clamp(0.0, 1.0)
}

/// Non-finite numbers become `0.0`.
fn finite(v: f64) -> f64 {
    if v.is_finite() { v } else { 0.0 }
}

#[cfg(test)]
mod tests;
