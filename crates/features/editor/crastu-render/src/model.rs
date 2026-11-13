//! The composition data model: plain, serde-able, no behaviour beyond validation.

use serde::{Deserialize, Serialize};

use crate::geom::{Rect, Rgba};
use crate::style::{CaptionStyle, PopIn};
use crate::text::WordTiming;

/// Why a composition is not usable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelError {
    EmptyFrame,
    BadFps,
    /// A time or size that is not finite, or a negative duration; the text says where.
    BadNumber(String),
    /// A word whose `end` is before its `start`; the text says where.
    BadWord(String),
}

impl std::fmt::Display for ModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyFrame => write!(f, "composition has a zero width or height"),
            Self::BadFps => write!(f, "composition fps must be at least 1"),
            Self::BadNumber(w) => write!(f, "not a finite, non-negative number: {w}"),
            Self::BadWord(w) => write!(f, "word ends before it starts: {w}"),
        }
    }
}

impl std::error::Error for ModelError {}

/// A video composition: a frame size, a frame rate, a length and layers drawn bottom to top.
///
/// All times are seconds on one timeline, including word timings and layer starts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Composition {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub duration: f64,
    pub layers: Vec<Layer>,
}

/// One layer, shown while `start <= t < start + duration`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub start: f64,
    pub duration: f64,
    /// Multiplies the opacity of everything the layer draws (`0.0..=1.0`).
    pub opacity: f64,
    pub kind: LayerKind,
}

/// What a layer draws.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LayerKind {
    /// Fills the whole frame.
    Solid {
        color: Rgba,
    },
    /// A reference to an image asset; decoding and drawing it is a later slice.
    Image {
        asset: String,
        area: Rect,
    },
    /// A reference to a video asset; `source_start` is the asset time shown at the layer's start.
    Video {
        asset: String,
        area: Rect,
        source_start: f64,
    },
    Title(TitleLayer),
    Caption(CaptionLayer),
}

/// A title: one block of text that can pop in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TitleLayer {
    pub text: String,
    /// Font size in frame pixels.
    pub font_size: f64,
    pub color: Rgba,
    /// Where the text sits, as fractions of the frame; lines are centred in it.
    pub area: Rect,
    /// Starts at the layer's start; the title fades in with the curve's progress.
    pub enter: Option<PopIn>,
}

/// Captions drawn from word timings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaptionLayer {
    /// In spoken order, `start` ascending, on the composition's timeline.
    pub words: Vec<WordTiming>,
    pub style: CaptionStyle,
}

impl Layer {
    /// A layer at full opacity.
    pub fn new(start: f64, duration: f64, kind: LayerKind) -> Self {
        Self {
            start,
            duration,
            opacity: 1.0,
            kind,
        }
    }

    /// Whether the layer is drawn at `t`.
    pub fn is_active(&self, t: f64) -> bool {
        t >= self.start && t < self.start + self.duration
    }
}

impl Composition {
    /// Checks sizes and times. Rendering never needs this to avoid a panic (bad numbers are
    /// treated as zero), but an editor should refuse such a composition.
    pub fn validate(&self) -> Result<(), ModelError> {
        if self.width == 0 || self.height == 0 {
            return Err(ModelError::EmptyFrame);
        }
        if self.fps == 0 {
            return Err(ModelError::BadFps);
        }
        let ok = |v: f64| v.is_finite() && v >= 0.0;
        if !ok(self.duration) {
            return Err(ModelError::BadNumber("composition duration".into()));
        }
        for (i, l) in self.layers.iter().enumerate() {
            if !ok(l.start) || !ok(l.duration) || !l.opacity.is_finite() {
                return Err(ModelError::BadNumber(format!(
                    "layer {i} start, duration or opacity"
                )));
            }
            if let LayerKind::Caption(c) = &l.kind {
                for (j, w) in c.words.iter().enumerate() {
                    if !ok(w.start) || !ok(w.end) {
                        return Err(ModelError::BadNumber(format!("layer {i} word {j}")));
                    }
                    if w.end < w.start {
                        return Err(ModelError::BadWord(format!("layer {i} word {j}")));
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comp(layers: Vec<Layer>) -> Composition {
        Composition {
            width: 1080,
            height: 1920,
            fps: 30,
            duration: 10.0,
            layers,
        }
    }

    #[test]
    fn validates_sizes_times_and_words() {
        let solid = LayerKind::Solid {
            color: Rgba::rgb(0, 0, 0),
        };
        assert_eq!(comp(vec![]).validate(), Ok(()));
        let mut c = comp(vec![]);
        c.width = 0;
        assert_eq!(c.validate(), Err(ModelError::EmptyFrame));
        let mut c = comp(vec![]);
        c.fps = 0;
        assert_eq!(c.validate(), Err(ModelError::BadFps));
        let c = comp(vec![Layer::new(f64::NAN, 1.0, solid.clone())]);
        assert!(matches!(c.validate(), Err(ModelError::BadNumber(_))));
        let words = vec![WordTiming::new("a", 2.0, 1.0)];
        let c = comp(vec![Layer::new(
            0.0,
            5.0,
            LayerKind::Caption(CaptionLayer {
                words,
                style: CaptionStyle::default(),
            }),
        )]);
        assert!(matches!(c.validate(), Err(ModelError::BadWord(_))));
    }

    #[test]
    fn layer_is_active_in_a_half_open_span() {
        let l = Layer::new(
            1.0,
            2.0,
            LayerKind::Solid {
                color: Rgba::rgb(1, 2, 3),
            },
        );
        assert!(!l.is_active(0.999));
        assert!(l.is_active(1.0));
        assert!(l.is_active(2.999));
        assert!(!l.is_active(3.0));
    }

    #[test]
    fn composition_json_round_trip() {
        let c = comp(vec![Layer::new(
            0.0,
            2.0,
            LayerKind::Title(TitleLayer {
                text: "Hi".into(),
                font_size: 80.0,
                color: Rgba::rgb(255, 255, 255),
                area: Rect::new(0.1, 0.1, 0.8, 0.2),
                enter: None,
            }),
        )]);
        let j = serde_json::to_string(&c).unwrap();
        assert_eq!(serde_json::from_str::<Composition>(&j).unwrap(), c);
    }
}
