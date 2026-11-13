//! Text layout seam and word timings.
//!
//! No fonts or glyphs live here. A [`TextLayout`] measures and breaks text; slice 2 puts Parley
//! behind it. [`FixedAdvance`] is the deterministic stand-in used by tests and previews.

use serde::{Deserialize, Serialize};
use std::ops::Range;

/// One spoken word with its time span in seconds (`start <= end`).
///
/// `crastu-captions` has no word type yet; this is the minimal shape the renderer needs and can
/// be replaced by a re-export when that crate defines one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WordTiming {
    pub text: String,
    pub start: f64,
    pub end: f64,
}

impl WordTiming {
    pub fn new(text: impl Into<String>, start: f64, end: f64) -> Self {
        Self {
            text: text.into(),
            start,
            end,
        }
    }
}

/// Size of a run of text.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextMetrics {
    pub width: f64,
    pub height: f64,
}

/// A line of wrapped text: the byte range of the source (without the space it broke at) and its
/// measured width.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub range: Range<usize>,
    pub width: f64,
}

/// Measures and wraps text. Implementations must be deterministic: the same arguments always give
/// the same result.
pub trait TextLayout {
    /// Identifies the layout (engine, font, version) in the frame cache key, so a change of font
    /// or engine never reuses a frame made with another.
    fn cache_id(&self) -> String;
    /// Size of `text` on one line at `font_size`.
    fn measure(&self, text: &str, font_size: f64) -> TextMetrics;
    /// Breaks `text` at spaces so no line is wider than `max_width` (a single word wider than it
    /// gets a line of its own).
    fn lines(&self, text: &str, font_size: f64, max_width: f64) -> Vec<Line>;
}

/// Every character is `advance` em wide and a line is `line_height` em high.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FixedAdvance {
    pub advance: f64,
    pub line_height: f64,
}

impl Default for FixedAdvance {
    fn default() -> Self {
        Self {
            advance: 0.5,
            line_height: 1.2,
        }
    }
}

impl TextLayout for FixedAdvance {
    fn cache_id(&self) -> String {
        format!(
            "fixed-advance:{:016x}:{:016x}",
            self.advance.to_bits(),
            self.line_height.to_bits()
        )
    }

    fn measure(&self, text: &str, font_size: f64) -> TextMetrics {
        TextMetrics {
            width: text.chars().count() as f64 * self.advance * font_size,
            height: self.line_height * font_size,
        }
    }

    fn lines(&self, text: &str, font_size: f64, max_width: f64) -> Vec<Line> {
        let mut lines = Vec::new();
        let mut current: Option<Range<usize>> = None;
        let mut pos = 0;
        for piece in text.split(' ') {
            let (ws, we) = (pos, pos + piece.len());
            pos = we + 1;
            if piece.is_empty() {
                continue;
            }
            match current.take() {
                None => current = Some(ws..we),
                Some(c) => {
                    if self.measure(&text[c.start..we], font_size).width <= max_width {
                        current = Some(c.start..we);
                    } else {
                        lines.push(self.line(text, c, font_size));
                        current = Some(ws..we);
                    }
                }
            }
        }
        if let Some(c) = current {
            lines.push(self.line(text, c, font_size));
        }
        lines
    }
}

impl FixedAdvance {
    fn line(&self, text: &str, range: Range<usize>, font_size: f64) -> Line {
        let width = self.measure(&text[range.clone()], font_size).width;
        Line { range, width }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_by_character_count() {
        let l = FixedAdvance::default();
        let m = l.measure("héllo", 10.0);
        assert_eq!(m.width, 25.0);
        assert_eq!(m.height, 12.0);
    }

    #[test]
    fn wraps_greedily_at_spaces() {
        let l = FixedAdvance {
            advance: 1.0,
            line_height: 1.0,
        };
        // 10 px per char; "aa bb" is 5 chars = 50.
        let lines = l.lines("aa bb cc ddddddd e", 10.0, 50.0);
        let texts: Vec<&str> = lines
            .iter()
            .map(|x| &"aa bb cc ddddddd e"[x.range.clone()])
            .collect();
        assert_eq!(texts, ["aa bb", "cc", "ddddddd", "e"]);
        assert_eq!(lines[0].width, 50.0);
        assert_eq!(lines[2].width, 70.0, "an over-long word keeps its own line");
    }

    #[test]
    fn empty_and_space_only_text_have_no_lines() {
        let l = FixedAdvance::default();
        assert!(l.lines("", 10.0, 100.0).is_empty());
        assert!(l.lines("   ", 10.0, 100.0).is_empty());
    }

    #[test]
    fn cache_id_changes_with_parameters() {
        let a = FixedAdvance::default().cache_id();
        let b = FixedAdvance {
            advance: 0.6,
            ..FixedAdvance::default()
        }
        .cache_id();
        assert_ne!(a, b);
    }
}
