//! Pure gain curves: every value is a function of an integer sample position.

/// Smooth 0..=1 ramp (`3t^2 - 2t^3`), zero slope at both ends so fades start and end without a kink.
#[must_use]
pub fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Piecewise linear gain over sample positions: constant before the first point and after the last,
/// a straight line between points. Empty means unity gain. Sample-accurate because the value is
/// computed from the position, never accumulated.
#[derive(Clone, Debug, PartialEq)]
pub struct Automation {
    points: Vec<(u64, f32)>,
}

impl Automation {
    /// A gain that never changes.
    #[must_use]
    pub fn constant(gain: f32) -> Self {
        Self {
            points: vec![(0, gain)],
        }
    }

    /// Points are `(sample position, linear gain)`; they are sorted by position. Two points at the
    /// same position make a step at that sample.
    #[must_use]
    pub fn new(mut points: Vec<(u64, f32)>) -> Self {
        points.sort_by_key(|p| p.0);
        Self { points }
    }

    /// Gain at `pos`.
    #[must_use]
    pub fn value_at(&self, pos: u64) -> f32 {
        let idx = self.points.partition_point(|p| p.0 <= pos);
        match (
            idx.checked_sub(1).map(|i| self.points[i]),
            self.points.get(idx),
        ) {
            (None, None) => 1.0,
            (None, Some(next)) => next.1,
            (Some(prev), None) => prev.1,
            (Some(prev), Some(next)) => {
                let t = (pos - prev.0) as f64 / (next.0 - prev.0) as f64;
                prev.1 + (next.1 - prev.1) * t as f32
            }
        }
    }

    /// True when every point is a finite number.
    #[must_use]
    pub fn is_finite(&self) -> bool {
        self.points.iter().all(|p| p.1.is_finite())
    }
}

impl Default for Automation {
    fn default() -> Self {
        Self::constant(1.0)
    }
}

/// Decibels to a linear gain.
#[must_use]
pub fn db_to_gain(db: f64) -> f32 {
    10f64.powf(db / 20.0) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automation_is_linear_and_clamped_at_the_ends() {
        let a = Automation::new(vec![(100, 1.0), (200, 0.0)]);
        assert_eq!(a.value_at(0), 1.0);
        assert_eq!(a.value_at(100), 1.0);
        assert!((a.value_at(150) - 0.5).abs() < 1e-6);
        assert_eq!(a.value_at(200), 0.0);
        assert_eq!(a.value_at(10_000), 0.0);
        assert_eq!(Automation::new(vec![]).value_at(5), 1.0);
    }

    #[test]
    fn smoothstep_hits_its_ends() {
        assert_eq!(smoothstep(0.0), 0.0);
        assert_eq!(smoothstep(1.0), 1.0);
        assert!((smoothstep(0.5) - 0.5).abs() < 1e-6);
    }
}
