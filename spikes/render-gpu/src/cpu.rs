//! The same composition drawn by `vello_cpu` (pure Rust, no GPU), as the deterministic fallback.

use crate::frame::Canvas;
use vello::kurbo::{Affine, Rect, RoundedRect, Shape};
use vello::peniko::{Color, FontData};
use vello_cpu::{Pixmap, RenderContext, RenderSettings, Resources};

pub struct Cpu {
    pub ctx: RenderContext,
    pub res: Resources,
    pub pixmap: Pixmap,
}

impl Cpu {
    pub fn new(w: u16, h: u16, threads: u16) -> Self {
        let settings = RenderSettings {
            num_threads: threads,
            ..RenderSettings::default()
        };
        Self {
            ctx: RenderContext::new_with(w, h, settings),
            res: Resources::new(),
            pixmap: Pixmap::new(w, h),
        }
    }

    /// Rasterizes the recorded scene and returns RGBA8 (premultiplied; opaque here).
    pub fn pixels(&mut self) -> &[u8] {
        self.ctx.flush();
        self.ctx.render(&mut self.pixmap, &mut self.res);
        self.pixmap.data_as_u8_slice()
    }
}

impl Canvas for Cpu {
    fn clear(&mut self) {
        self.ctx.reset();
    }
    fn fill_rect(&mut self, rect: Rect, color: Color) {
        self.ctx.reset_transform();
        self.ctx.set_paint(color);
        self.ctx.fill_rect(&rect);
    }
    fn fill_rrect(&mut self, rect: RoundedRect, color: Color) {
        self.ctx.reset_transform();
        self.ctx.set_paint(color);
        self.ctx.fill_path(&rect.to_path(0.1));
    }
    fn glyphs(
        &mut self,
        font: &FontData,
        size: f32,
        xf: Affine,
        color: Color,
        glyphs: &[vello::Glyph],
    ) {
        self.ctx.set_transform(xf);
        self.ctx.set_paint(color);
        let it = glyphs.iter().map(|g| vello_cpu::Glyph {
            id: g.id,
            x: g.x,
            y: g.y,
        });
        let _ = self
            .ctx
            .glyph_run(&mut self.res, font)
            .font_size(size)
            .fill_glyphs(it);
    }
    fn push_clip(&mut self, xf: Affine, rect: Rect) {
        self.ctx.set_transform(xf);
        self.ctx.push_clip_layer(&rect.to_path(0.1));
    }
    fn pop_clip(&mut self) {
        self.ctx.pop_layer();
    }
}
