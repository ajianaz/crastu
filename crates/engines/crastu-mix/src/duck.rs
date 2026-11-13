//! Sidechain ducking: the voice bus lowers the music bus.
//!
//! The sidechain is the voice activity of the timeline (the merged voice clips), not an envelope
//! follower on the rendered signal. That makes the duck a pure function of the sample position,
//! so preview and export agree and any block size gives the same bits. It also lets the music
//! start falling `attack` samples before the voice begins (the timeline is known ahead), which a
//! follower cannot do. Spans from a speech detector can feed [`Ducker::new`] the same way.

use crate::curve::smoothstep;

/// Duck gain over time from sorted, merged voice spans.
#[derive(Clone, Debug)]
pub struct Ducker {
    spans: Vec<(u64, u64)>,
    floor: f32,
    attack: u64,
    release: u64,
}

impl Ducker {
    /// `spans` are sorted, non-overlapping `(start, end)` pairs (see `Timeline::voice_spans`);
    /// `floor` is the linear gain of the music while the voice speaks (0.25 is -12 dB); the music
    /// is fully down at the first voice sample and starts falling `attack` samples earlier, and
    /// it takes `release` samples to come back after the voice ends.
    #[must_use]
    pub fn new(spans: Vec<(u64, u64)>, floor: f32, attack: u64, release: u64) -> Self {
        Self {
            spans,
            floor: floor.clamp(0.0, 1.0),
            attack,
            release,
        }
    }

    /// Fraction of ducking at `pos`, 0 (none) to 1 (full).
    fn amount(&self, pos: u64) -> f32 {
        let idx = self.spans.partition_point(|s| s.1 <= pos);
        let mut amount = 0.0f32;
        if let Some(&(start, _)) = self.spans.get(idx) {
            if pos >= start {
                amount = 1.0;
            } else if start - pos < self.attack {
                amount = smoothstep(1.0 - (start - pos) as f32 / self.attack as f32);
            }
        }
        if let Some(&(_, end)) = idx.checked_sub(1).and_then(|i| self.spans.get(i)) {
            let since = pos - end;
            if since < self.release {
                amount = amount.max(smoothstep(1.0 - since as f32 / self.release as f32));
            }
        }
        amount
    }

    /// Linear gain to apply to the music at `pos`.
    #[must_use]
    pub fn gain_at(&self, pos: u64) -> f32 {
        1.0 - (1.0 - self.floor) * self.amount(pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duck_is_full_inside_a_span_and_ramps_around_it() {
        let d = Ducker::new(vec![(1000, 2000)], 0.25, 100, 400);
        assert_eq!(d.gain_at(0), 1.0);
        assert_eq!(d.gain_at(900), 1.0);
        assert!(d.gain_at(950) < 1.0 && d.gain_at(950) > 0.25);
        assert_eq!(d.gain_at(1000), 0.25);
        assert_eq!(d.gain_at(1999), 0.25);
        assert_eq!(d.gain_at(2000), 0.25);
        assert!(d.gain_at(2200) > 0.25 && d.gain_at(2200) < 1.0);
        assert_eq!(d.gain_at(2400), 1.0);
    }

    #[test]
    fn duck_is_continuous_between_close_spans() {
        let d = Ducker::new(vec![(1000, 2000), (2100, 3000)], 0.25, 300, 300);
        let mut worst = 0.0f32;
        for p in 900..3400 {
            worst = worst.max((d.gain_at(p) - d.gain_at(p + 1)).abs());
        }
        assert!(worst < 0.02, "step {worst}");
        assert!(d.gain_at(2050) < 1.0);
    }
}
