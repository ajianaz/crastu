//! The value logic of the slider and the range slider, with no drawing.

/// A move of the value from the keyboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nudge {
    Down,
    Up,
    /// Ten steps down (Page Down) or up (Page Up).
    BigDown,
    BigUp,
    Min,
    Max,
}

impl Nudge {
    pub fn from_key(key: &str) -> Option<Nudge> {
        match key {
            "left" | "down" => Some(Nudge::Down),
            "right" | "up" => Some(Nudge::Up),
            "pagedown" => Some(Nudge::BigDown),
            "pageup" => Some(Nudge::BigUp),
            "home" => Some(Nudge::Min),
            "end" => Some(Nudge::Max),
            _ => None,
        }
    }
}

/// A single value between `min` and `max`, moving in `step`s.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slider {
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub value: f32,
}

impl Slider {
    pub fn new(min: f32, max: f32, step: f32, value: f32) -> Slider {
        let mut s = Slider {
            min,
            max,
            step,
            value,
        };
        s.value = s.snap(value);
        s
    }

    /// Where the value is, from 0 (at `min`) to 1 (at `max`).
    pub fn fraction(&self) -> f32 {
        fraction(self.min, self.max, self.value)
    }

    /// The value nearest to `v` that is allowed: on a step and inside the range.
    pub fn snap(&self, v: f32) -> f32 {
        snap(self.min, self.max, self.step, v)
    }

    /// Sets the value from a position along the track, 0 to 1 (a click or a drag).
    pub fn set_fraction(&mut self, f: f32) {
        self.value = self.snap(self.min + f.clamp(0.0, 1.0) * (self.max - self.min));
    }

    pub fn nudge(&mut self, n: Nudge) {
        self.value = nudged(self.min, self.max, self.step, self.value, n);
    }
}

/// Which end of a range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Thumb {
    Low,
    High,
}

/// Two values; the low one never passes the high one and they keep `min_gap` apart.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RangeSlider {
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub min_gap: f32,
    pub low: f32,
    pub high: f32,
}

impl RangeSlider {
    pub fn new(min: f32, max: f32, step: f32, min_gap: f32, low: f32, high: f32) -> RangeSlider {
        let mut r = RangeSlider {
            min,
            max,
            step,
            min_gap,
            low,
            high,
        };
        r.set(Thumb::Low, low);
        r.set(Thumb::High, high);
        r
    }

    pub fn get(&self, thumb: Thumb) -> f32 {
        match thumb {
            Thumb::Low => self.low,
            Thumb::High => self.high,
        }
    }

    pub fn set(&mut self, thumb: Thumb, v: f32) {
        let v = snap(self.min, self.max, self.step, v);
        match thumb {
            Thumb::Low => self.low = v.min(self.high - self.min_gap).max(self.min),
            Thumb::High => self.high = v.max(self.low + self.min_gap).min(self.max),
        }
    }

    pub fn nudge(&mut self, thumb: Thumb, n: Nudge) {
        let v = nudged(self.min, self.max, self.step, self.get(thumb), n);
        self.set(thumb, v);
    }

    pub fn fraction(&self, thumb: Thumb) -> f32 {
        fraction(self.min, self.max, self.get(thumb))
    }

    /// The thumb a press at `f` (0 to 1) should take: the closer one, and the low one on a tie
    /// unless both sit at the top.
    pub fn nearest(&self, f: f32) -> Thumb {
        let low = (self.fraction(Thumb::Low) - f).abs();
        let high = (self.fraction(Thumb::High) - f).abs();
        if low < high || (low == high && f < 1.0) {
            Thumb::Low
        } else {
            Thumb::High
        }
    }

    pub fn set_fraction(&mut self, thumb: Thumb, f: f32) {
        self.set(thumb, self.min + f.clamp(0.0, 1.0) * (self.max - self.min));
    }
}

fn fraction(min: f32, max: f32, v: f32) -> f32 {
    if max > min {
        ((v - min) / (max - min)).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn snap(min: f32, max: f32, step: f32, v: f32) -> f32 {
    let v = v.clamp(min, max);
    if step <= 0.0 {
        return v;
    }
    (min + ((v - min) / step).round() * step).clamp(min, max)
}

fn nudged(min: f32, max: f32, step: f32, v: f32, n: Nudge) -> f32 {
    let big = step * 10.0;
    let target = match n {
        Nudge::Down => v - step,
        Nudge::Up => v + step,
        Nudge::BigDown => v - big,
        Nudge::BigUp => v + big,
        Nudge::Min => min,
        Nudge::Max => max,
    };
    snap(min, max, step, target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn volume() -> Slider {
        Slider::new(-60.0, 6.0, 0.5, -6.0)
    }

    #[test]
    fn a_new_slider_snaps_into_range() {
        assert_eq!(Slider::new(0.0, 10.0, 1.0, 99.0).value, 10.0);
        assert_eq!(Slider::new(0.0, 10.0, 1.0, 3.4).value, 3.0);
        assert_eq!(Slider::new(0.0, 10.0, 0.0, 3.4).value, 3.4);
    }

    #[test]
    fn fraction_and_position_round_trip() {
        let mut s = volume();
        assert!((s.fraction() - 54.0 / 66.0).abs() < 1e-6);
        s.set_fraction(0.0);
        assert_eq!(s.value, -60.0);
        s.set_fraction(1.0);
        assert_eq!(s.value, 6.0);
        s.set_fraction(7.0);
        assert_eq!(s.value, 6.0);
        s.set_fraction(0.5);
        assert_eq!(s.value, -27.0);
        assert_eq!(Slider::new(1.0, 1.0, 1.0, 1.0).fraction(), 0.0);
    }

    #[test]
    fn keys_move_by_a_step_ten_steps_or_to_the_ends() {
        let mut s = volume();
        s.nudge(Nudge::Up);
        assert_eq!(s.value, -5.5);
        s.nudge(Nudge::BigDown);
        assert_eq!(s.value, -10.5);
        s.nudge(Nudge::Max);
        s.nudge(Nudge::Up);
        assert_eq!(s.value, 6.0);
        s.nudge(Nudge::Min);
        s.nudge(Nudge::Down);
        assert_eq!(s.value, -60.0);
        assert_eq!(Nudge::from_key("pageup"), Some(Nudge::BigUp));
        assert_eq!(Nudge::from_key("x"), None);
    }

    fn clip() -> RangeSlider {
        RangeSlider::new(5.0, 90.0, 1.0, 5.0, 30.0, 60.0)
    }

    #[test]
    fn the_thumbs_never_cross_and_keep_the_gap() {
        let mut r = clip();
        r.set(Thumb::Low, 80.0);
        assert_eq!(r.low, 55.0);
        r.set(Thumb::High, 10.0);
        assert_eq!(r.high, 60.0);
        r.set(Thumb::Low, 55.0);
        r.set(Thumb::High, 0.0);
        assert_eq!(r.high, 60.0);
        r.set(Thumb::High, 500.0);
        assert_eq!(r.high, 90.0);
        r.set(Thumb::Low, -5.0);
        assert_eq!(r.low, 5.0);
    }

    #[test]
    fn a_press_takes_the_nearer_thumb() {
        let r = clip();
        assert_eq!(r.nearest(0.0), Thumb::Low);
        assert_eq!(r.nearest(1.0), Thumb::High);
        assert_eq!(r.nearest(0.5), Thumb::High);
        let both_top = RangeSlider::new(0.0, 10.0, 1.0, 0.0, 10.0, 10.0);
        assert_eq!(both_top.nearest(1.0), Thumb::High);
        assert_eq!(both_top.nearest(0.2), Thumb::Low);
    }

    #[test]
    fn range_keys_and_drag_use_the_same_limits() {
        let mut r = clip();
        r.nudge(Thumb::High, Nudge::Down);
        assert_eq!(r.high, 59.0);
        r.nudge(Thumb::Low, Nudge::Max);
        assert_eq!(r.low, 54.0);
        r.set_fraction(Thumb::High, 1.0);
        assert_eq!(r.high, 90.0);
    }
}
