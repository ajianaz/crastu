//! Small geometry and colour types shared by the model and the painters.

use serde::{Deserialize, Serialize};

/// A rectangle: top-left corner and size.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub const fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }

    /// This rectangle scaled about its centre (the origin of a pop-in).
    pub fn scaled_about_centre(self, scale: f64) -> Self {
        let (cx, cy) = (self.x + self.w / 2.0, self.y + self.h / 2.0);
        let (w, h) = (self.w * scale, self.h * scale);
        Self::new(cx - w / 2.0, cy - h / 2.0, w, h)
    }

    /// The overlap of two rectangles; zero-sized when they do not touch.
    pub fn intersect(self, o: Self) -> Self {
        let x0 = self.x.max(o.x);
        let y0 = self.y.max(o.y);
        let x1 = (self.x + self.w).min(o.x + o.w);
        let y1 = (self.y + self.h).min(o.y + o.h);
        Self::new(x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0))
    }
}

/// An 8-bit RGBA colour, straight (not premultiplied) alpha.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::new(r, g, b, 255)
    }
}

/// Non-finite numbers become `0.0` so one bad value cannot poison a frame or a cache key.
pub(crate) fn finite(v: f64) -> f64 {
    if v.is_finite() { v } else { 0.0 }
}
