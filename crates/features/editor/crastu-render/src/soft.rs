//! A tiny software RGBA rasterizer for tests: rectangles only, no anti-aliasing.
//!
//! Edges round to the nearest pixel, blending is integer arithmetic, so the same calls give the
//! same bytes on every machine. Text runs are drawn as solid blocks of their (scaled) box, so a
//! test can see where a word is, how big and in which colour; assets are drawn as a flat
//! placeholder. The frame starts opaque black and stays opaque.

use crate::Composition;
use crate::geom::{Rect, Rgba};
use crate::painter::{AssetKind, Painter, TextRun};
use crate::render::render_into;
use crate::text::TextLayout;

/// The largest frame [`SoftFrame::new`] allocates (16384 x 16384).
pub const MAX_PIXELS: u64 = 16384 * 16384;

/// An RGBA8 frame, row-major, top row first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoftFrame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    clips: Vec<PixelRect>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PixelRect {
    x0: i64,
    y0: i64,
    x1: i64,
    y1: i64,
}

impl PixelRect {
    fn of(r: Rect) -> Self {
        let edge = |v: f64| {
            if v.is_finite() {
                v.round().clamp(-1e9, 1e9) as i64
            } else {
                0
            }
        };
        let (x0, y0) = (edge(r.x), edge(r.y));
        Self {
            x0,
            y0,
            x1: edge(r.x + r.w).max(x0),
            y1: edge(r.y + r.h).max(y0),
        }
    }

    fn intersect(self, o: Self) -> Self {
        let x0 = self.x0.max(o.x0);
        let y0 = self.y0.max(o.y0);
        Self {
            x0,
            y0,
            x1: self.x1.min(o.x1).max(x0),
            y1: self.y1.min(o.y1).max(y0),
        }
    }
}

impl SoftFrame {
    /// An opaque black frame. A frame of more than [`MAX_PIXELS`] pixels is returned empty (0 by
    /// 0) instead of allocating.
    pub fn new(width: u32, height: u32) -> Self {
        let (width, height) = if u64::from(width) * u64::from(height) > MAX_PIXELS {
            (0, 0)
        } else {
            (width, height)
        };
        let n = (width as usize).saturating_mul(height as usize);
        let pixels = [0u8, 0, 0, 255].repeat(n);
        Self {
            width,
            height,
            pixels,
            clips: Vec::new(),
        }
    }

    /// Renders `comp` at `t` into a new frame.
    pub fn render(comp: &Composition, t: f64, layout: &dyn TextLayout) -> Self {
        let mut f = Self::new(comp.width, comp.height);
        render_into(comp, t, layout, &mut f);
        f
    }

    /// The RGBA value at a pixel; `None` outside the frame.
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let i = (y as usize * self.width as usize + x as usize) * 4;
        self.pixels.get(i..i + 4)?.try_into().ok()
    }

    fn fill(&mut self, rect: Rect, color: Rgba, opacity: f64) {
        let bounds = PixelRect {
            x0: 0,
            y0: 0,
            x1: i64::from(self.width),
            y1: i64::from(self.height),
        };
        let mut r = PixelRect::of(rect).intersect(bounds);
        if let Some(c) = self.clips.last() {
            r = r.intersect(*c);
        }
        let op = if opacity.is_finite() {
            opacity.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let a = (f64::from(color.a) * op).round() as u32;
        if a == 0 {
            return;
        }
        let src = [u32::from(color.r), u32::from(color.g), u32::from(color.b)];
        for y in r.y0..r.y1 {
            for x in r.x0..r.x1 {
                let i = (y as usize * self.width as usize + x as usize) * 4;
                for (c, s) in src.iter().enumerate() {
                    let d = u32::from(self.pixels[i + c]);
                    self.pixels[i + c] = ((s * a + d * (255 - a) + 127) / 255) as u8;
                }
            }
        }
    }
}

impl Painter for SoftFrame {
    fn draw_rect(&mut self, rect: Rect, color: Rgba, opacity: f64) {
        self.fill(rect, color, opacity);
    }

    fn draw_asset(&mut self, kind: AssetKind, _asset: &str, rect: Rect, _t: f64, opacity: f64) {
        let color = match kind {
            AssetKind::Image => Rgba::rgb(90, 90, 90),
            AssetKind::Video => Rgba::rgb(60, 60, 100),
        };
        self.fill(rect, color, opacity);
    }

    fn draw_text_run(&mut self, run: &TextRun) {
        self.fill(
            run.rect.scaled_about_centre(run.scale),
            run.color,
            run.opacity,
        );
    }

    fn push_clip(&mut self, rect: Rect) {
        let mut r = PixelRect::of(rect);
        if let Some(c) = self.clips.last() {
            r = r.intersect(*c);
        }
        self.clips.push(r);
    }

    fn pop_clip(&mut self) {
        self.clips.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_clips_and_blends() {
        let mut f = SoftFrame::new(10, 10);
        f.draw_rect(Rect::new(2.0, 2.0, 4.0, 4.0), Rgba::rgb(255, 0, 0), 1.0);
        assert_eq!(f.pixel(2, 2), Some([255, 0, 0, 255]));
        assert_eq!(f.pixel(5, 5), Some([255, 0, 0, 255]));
        assert_eq!(f.pixel(6, 6), Some([0, 0, 0, 255]));
        f.push_clip(Rect::new(0.0, 0.0, 3.0, 10.0));
        f.draw_rect(Rect::new(0.0, 0.0, 10.0, 1.0), Rgba::rgb(0, 255, 0), 1.0);
        f.pop_clip();
        assert_eq!(f.pixel(2, 0), Some([0, 255, 0, 255]));
        assert_eq!(f.pixel(3, 0), Some([0, 0, 0, 255]));
        f.draw_rect(Rect::new(8.0, 8.0, 2.0, 2.0), Rgba::rgb(255, 255, 255), 0.5);
        assert_eq!(f.pixel(8, 8), Some([128, 128, 128, 255]));
        assert_eq!(f.pixel(10, 0), None);
    }

    #[test]
    fn out_of_bounds_and_non_finite_rects_do_not_panic() {
        let mut f = SoftFrame::new(4, 4);
        f.draw_rect(
            Rect::new(-50.0, -50.0, 500.0, 500.0),
            Rgba::rgb(1, 2, 3),
            1.0,
        );
        f.draw_rect(
            Rect::new(f64::NAN, 0.0, f64::INFINITY, 1.0),
            Rgba::rgb(9, 9, 9),
            1.0,
        );
        f.draw_rect(Rect::new(0.0, 0.0, 4.0, 4.0), Rgba::rgb(9, 9, 9), f64::NAN);
        f.pop_clip();
        assert_eq!(f.pixel(0, 0), Some([1, 2, 3, 255]));
    }

    #[test]
    fn nested_clips_intersect() {
        let mut f = SoftFrame::new(10, 1);
        f.push_clip(Rect::new(0.0, 0.0, 6.0, 1.0));
        f.push_clip(Rect::new(4.0, 0.0, 6.0, 1.0));
        f.draw_rect(
            Rect::new(0.0, 0.0, 10.0, 1.0),
            Rgba::rgb(255, 255, 255),
            1.0,
        );
        assert_eq!(f.pixel(3, 0), Some([0, 0, 0, 255]));
        assert_eq!(f.pixel(4, 0), Some([255, 255, 255, 255]));
        assert_eq!(f.pixel(5, 0), Some([255, 255, 255, 255]));
        assert_eq!(f.pixel(6, 0), Some([0, 0, 0, 255]));
    }

    #[test]
    fn a_frame_over_the_cap_is_empty() {
        let f = SoftFrame::new(u32::MAX, u32::MAX);
        assert_eq!((f.width, f.height, f.pixels.len()), (0, 0, 0));
    }
}
