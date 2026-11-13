//! The drawing seam. [`render_into`](crate::render_into) talks only to a [`Painter`]; the display
//! list recorder here and the software rasterizer in [`soft`](crate::soft) implement it, and
//! slice 2 adds a GPU one.

use crate::geom::{Rect, Rgba};

/// What an asset layer refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AssetKind {
    Image,
    Video,
}

/// A run of text: the box the text occupies before scaling, drawn scaled about the box centre.
#[derive(Debug, Clone, PartialEq)]
pub struct TextRun {
    pub text: String,
    pub rect: Rect,
    /// Font size in frame pixels.
    pub font_size: f64,
    pub color: Rgba,
    pub scale: f64,
    pub opacity: f64,
}

/// Draws one frame. Coordinates are frame pixels with the origin at the top left; opacity is
/// `0.0..=1.0` and multiplies the colour's own alpha.
pub trait Painter {
    fn draw_rect(&mut self, rect: Rect, color: Rgba, opacity: f64);
    /// An image or video asset at `rect`; `source_time` is the asset time to show (`0.0` for an
    /// image).
    fn draw_asset(
        &mut self,
        kind: AssetKind,
        asset: &str,
        rect: Rect,
        source_time: f64,
        opacity: f64,
    );
    fn draw_text_run(&mut self, run: &TextRun);
    /// Limits what is drawn until the matching [`pop_clip`](Painter::pop_clip); clips nest by
    /// intersection.
    fn push_clip(&mut self, rect: Rect);
    fn pop_clip(&mut self);
}

/// One recorded drawing call.
#[derive(Debug, Clone, PartialEq)]
pub enum Primitive {
    Rect {
        rect: Rect,
        color: Rgba,
        opacity: f64,
    },
    Asset {
        kind: AssetKind,
        asset: String,
        rect: Rect,
        source_time: f64,
        opacity: f64,
    },
    Text(TextRun),
    PushClip(Rect),
    PopClip,
}

/// A [`Painter`] that records the ordered primitives of a frame.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DisplayList {
    pub primitives: Vec<Primitive>,
}

impl Painter for DisplayList {
    fn draw_rect(&mut self, rect: Rect, color: Rgba, opacity: f64) {
        self.primitives.push(Primitive::Rect {
            rect,
            color,
            opacity,
        });
    }

    fn draw_asset(
        &mut self,
        kind: AssetKind,
        asset: &str,
        rect: Rect,
        source_time: f64,
        opacity: f64,
    ) {
        self.primitives.push(Primitive::Asset {
            kind,
            asset: asset.to_owned(),
            rect,
            source_time,
            opacity,
        });
    }

    fn draw_text_run(&mut self, run: &TextRun) {
        self.primitives.push(Primitive::Text(run.clone()));
    }

    fn push_clip(&mut self, rect: Rect) {
        self.primitives.push(Primitive::PushClip(rect));
    }

    fn pop_clip(&mut self) {
        self.primitives.push(Primitive::PopClip);
    }
}
