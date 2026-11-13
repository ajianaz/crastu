//! Styles as data: serde, versioned, editable from the inspector and MCP.

use serde::{Deserialize, Serialize};

use crate::geom::{Rect, Rgba};
use crastu_motion::Curve;

/// The version written into every [`CaptionStyle`]. Bump it when a field changes meaning.
pub const STYLE_VERSION: u32 = 1;

/// Why a style could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StyleError {
    /// Not valid JSON for a style (the text is serde's message).
    Parse(String),
    /// The style was written by a newer version, or has version 0.
    Version(u32),
}

impl std::fmt::Display for StyleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(m) => write!(f, "caption style does not parse: {m}"),
            Self::Version(v) => write!(
                f,
                "caption style version {v} is not supported (this build reads 1..={STYLE_VERSION})"
            ),
        }
    }
}

impl std::error::Error for StyleError {}

/// Scale (and, for titles, opacity) that settles to normal along a curve.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PopIn {
    /// Scale at the start of the animation; `1.0` at the end of it.
    pub from_scale: f64,
    pub curve: Curve,
}

impl PopIn {
    /// `(scale, progress)` after `elapsed` seconds. The scale may overshoot `1.0` on a spring or
    /// back curve; `progress` is the raw curve value.
    pub fn at(self, elapsed: f64) -> (f64, f64) {
        let p = self.curve.at(elapsed);
        (self.from_scale + (1.0 - self.from_scale) * p, p)
    }
}

/// The word being spoken takes this colour.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Highlight {
    pub color: Rgba,
}

/// The word fills with this colour from left to right over its time span; words already spoken
/// stay filled.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Karaoke {
    pub fill: Rgba,
}

/// How a caption layer looks and animates. All animation is derived from word timings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptionStyle {
    pub version: u32,
    /// Font size in frame pixels.
    pub font_size: f64,
    pub color: Rgba,
    /// Where the text sits, as fractions of the frame (`0.0..=1.0`); lines are centred in it.
    pub area: Rect,
    /// Words shown together; the next page replaces the last one at its first word.
    pub words_per_page: usize,
    /// How long a page stays after its last word ends, in seconds.
    pub linger: f64,
    /// Opacity of words not yet spoken (`0.0` hides them).
    pub unspoken_opacity: f64,
    pub highlight: Option<Highlight>,
    pub pop_in: Option<PopIn>,
    pub karaoke: Option<Karaoke>,
}

impl Default for CaptionStyle {
    fn default() -> Self {
        Self {
            version: STYLE_VERSION,
            font_size: 64.0,
            color: Rgba::rgb(255, 255, 255),
            area: Rect::new(0.1, 0.65, 0.8, 0.2),
            words_per_page: 6,
            linger: 0.3,
            unspoken_opacity: 1.0,
            highlight: None,
            pop_in: None,
            karaoke: None,
        }
    }
}

impl CaptionStyle {
    /// Reads a style from JSON. Unknown fields and versions newer than [`STYLE_VERSION`] are
    /// errors, never silently dropped.
    pub fn from_json(json: &str) -> Result<Self, StyleError> {
        let s: Self = serde_json::from_str(json).map_err(|e| StyleError::Parse(e.to_string()))?;
        if s.version == 0 || s.version > STYLE_VERSION {
            return Err(StyleError::Version(s.version));
        }
        Ok(s)
    }

    /// The style as JSON.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crastu_motion::{Easing, Spring};

    fn sample() -> CaptionStyle {
        CaptionStyle {
            highlight: Some(Highlight {
                color: Rgba::rgb(76, 194, 176),
            }),
            pop_in: Some(PopIn {
                from_scale: 0.6,
                curve: Curve::Spring {
                    spring: Spring::new(0.4, 0.3),
                },
            }),
            karaoke: Some(Karaoke {
                fill: Rgba::rgb(255, 220, 0),
            }),
            ..CaptionStyle::default()
        }
    }

    #[test]
    fn json_round_trip() {
        let s = sample();
        assert_eq!(CaptionStyle::from_json(&s.to_json()).unwrap(), s);
    }

    #[test]
    fn rejects_newer_and_zero_versions_and_unknown_fields() {
        let mut s = CaptionStyle {
            version: STYLE_VERSION + 1,
            ..CaptionStyle::default()
        };
        assert_eq!(
            CaptionStyle::from_json(&s.to_json()),
            Err(StyleError::Version(STYLE_VERSION + 1))
        );
        s.version = 0;
        assert_eq!(
            CaptionStyle::from_json(&s.to_json()),
            Err(StyleError::Version(0))
        );
        let with_extra = CaptionStyle::default()
            .to_json()
            .replace('{', "{\"future\":1,");
        assert!(matches!(
            CaptionStyle::from_json(&with_extra),
            Err(StyleError::Parse(_))
        ));
    }

    #[test]
    fn pop_in_reaches_one() {
        let p = PopIn {
            from_scale: 0.5,
            curve: Curve::Ease {
                easing: Easing::CubicOut,
                duration: 0.2,
            },
        };
        assert_eq!(p.at(0.0), (0.5, 0.0));
        assert_eq!(p.at(0.2), (1.0, 1.0));
        assert_eq!(p.at(5.0), (1.0, 1.0));
    }
}
