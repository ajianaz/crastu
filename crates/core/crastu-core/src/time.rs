use std::fmt;

use serde::{Deserialize, Serialize};

/// A point or span on a timeline, in microseconds. Negative values are allowed for offsets.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Time(i64);

/// A video frame rate as a rational number, for example 30000/1001.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FrameRate {
    pub num: u32,
    pub den: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TimecodeError {
    #[error("expected hh:mm:ss.ff, got {0:?}")]
    Format(String),
    #[error("frame {frame} is out of range for {fps} fps")]
    Frame { frame: u32, fps: u32 },
}

impl FrameRate {
    pub const FPS_24: FrameRate = FrameRate { num: 24, den: 1 };
    pub const FPS_25: FrameRate = FrameRate { num: 25, den: 1 };
    pub const FPS_30: FrameRate = FrameRate { num: 30, den: 1 };
    pub const FPS_60: FrameRate = FrameRate { num: 60, den: 1 };

    /// Whole frames per second used for the frame field of a timecode (29.97 shows as 30).
    pub fn nominal(&self) -> u32 {
        (self.num + self.den / 2) / self.den.max(1)
    }

    fn micros_per_frame(&self) -> f64 {
        1_000_000.0 * f64::from(self.den) / f64::from(self.num.max(1))
    }

    /// Frame index for `micros`. Times are stored in whole microseconds, so a frame
    /// boundary can land a hair below the integer; the tolerance absorbs that.
    fn frame_of(&self, micros: f64) -> i64 {
        (micros / self.micros_per_frame() + 1e-3).floor() as i64
    }
}

impl Time {
    pub const ZERO: Time = Time(0);

    pub const fn from_micros(us: i64) -> Self {
        Time(us)
    }

    pub const fn from_millis(ms: i64) -> Self {
        Time(ms * 1_000)
    }

    pub fn from_secs_f64(s: f64) -> Self {
        Time((s * 1_000_000.0).round() as i64)
    }

    pub const fn as_micros(&self) -> i64 {
        self.0
    }

    pub fn as_secs_f64(&self) -> f64 {
        self.0 as f64 / 1_000_000.0
    }

    /// The frame index that contains this time at `rate`.
    pub fn frame(&self, rate: FrameRate) -> i64 {
        rate.frame_of(self.0 as f64)
    }

    /// Formats as `hh:mm:ss.ff` (frames), the timecode style used across the UI.
    pub fn to_timecode(&self, rate: FrameRate) -> String {
        let sign = if self.0 < 0 { "-" } else { "" };
        let us = self.0.unsigned_abs();
        let total_secs = us / 1_000_000;
        let rem = (us % 1_000_000) as f64;
        let frame = (rate.frame_of(rem) as u64).min(u64::from(rate.nominal().saturating_sub(1)));
        format!(
            "{sign}{:02}:{:02}:{:02}.{:02}",
            total_secs / 3600,
            (total_secs / 60) % 60,
            total_secs % 60,
            frame
        )
    }

    /// Parses `hh:mm:ss.ff` or `mm:ss.ff` (frames at `rate`).
    pub fn parse_timecode(s: &str, rate: FrameRate) -> Result<Time, TimecodeError> {
        let bad = || TimecodeError::Format(s.to_string());
        let (neg, body) = match s.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, s),
        };
        let (hms, frames) = body.split_once('.').ok_or_else(bad)?;
        let parts: Vec<&str> = hms.split(':').collect();
        let nums: Vec<u64> = parts
            .iter()
            .map(|p| p.parse::<u64>())
            .collect::<Result<_, _>>()
            .map_err(|_| bad())?;
        let (h, m, sec) = match nums.as_slice() {
            [h, m, s] => (*h, *m, *s),
            [m, s] => (0, *m, *s),
            _ => return Err(bad()),
        };
        if m >= 60 || sec >= 60 {
            return Err(bad());
        }
        let frame: u32 = frames.parse().map_err(|_| bad())?;
        if frame >= rate.nominal() {
            return Err(TimecodeError::Frame {
                frame,
                fps: rate.nominal(),
            });
        }
        let us = ((h * 3600 + m * 60 + sec) * 1_000_000) as f64
            + f64::from(frame) * rate.micros_per_frame();
        let us = us.round() as i64;
        Ok(Time(if neg { -us } else { us }))
    }
}

impl std::ops::Add for Time {
    type Output = Time;
    fn add(self, rhs: Time) -> Time {
        Time(self.0 + rhs.0)
    }
}

impl std::ops::Sub for Time {
    type Output = Time;
    fn sub(self, rhs: Time) -> Time {
        Time(self.0 - rhs.0)
    }
}

impl fmt::Display for Time {
    /// `mm:ss.mmm`, for logs and tooltips.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.0 < 0 { "-" } else { "" };
        let ms = self.0.unsigned_abs() / 1_000;
        write!(
            f,
            "{sign}{:02}:{:02}.{:03}",
            ms / 60_000,
            (ms / 1_000) % 60,
            ms % 1_000
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_timecode_with_frames() {
        let t = Time::from_secs_f64(12.0 * 60.0 + 4.0 + 18.0 / 30.0);
        assert_eq!(t.to_timecode(FrameRate::FPS_30), "00:12:04.18");
        assert_eq!(
            Time::from_secs_f64(3725.5).to_timecode(FrameRate::FPS_25),
            "01:02:05.12"
        );
    }

    #[test]
    fn parses_what_it_formats() {
        for rate in [
            FrameRate::FPS_24,
            FrameRate::FPS_30,
            FrameRate {
                num: 30000,
                den: 1001,
            },
        ] {
            for secs in [0.0, 1.5, 59.9, 724.6, 3599.0] {
                let t = Time::from_secs_f64(secs);
                let tc = t.to_timecode(rate);
                let back = Time::parse_timecode(&tc, rate).unwrap();
                assert_eq!(back.to_timecode(rate), tc, "rate {rate:?} secs {secs}");
            }
        }
    }

    #[test]
    fn rejects_bad_timecodes() {
        assert!(Time::parse_timecode("12:04", FrameRate::FPS_30).is_err());
        assert!(Time::parse_timecode("00:61:00.00", FrameRate::FPS_30).is_err());
        assert_eq!(
            Time::parse_timecode("00:00:01.30", FrameRate::FPS_30),
            Err(TimecodeError::Frame { frame: 30, fps: 30 })
        );
    }

    #[test]
    fn short_form_and_negative() {
        let t = Time::parse_timecode("-01:30.00", FrameRate::FPS_30).unwrap();
        assert_eq!(t, Time::from_millis(-90_000));
        assert_eq!(t.to_string(), "-01:30.000");
    }
}
