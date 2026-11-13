//! The test composition: static rounded rects plus karaoke captions, a pure function of `t`.

use crate::gpu::{H, W};
use parley::{
    FontContext, FontFamily, FontWeight, Layout, LayoutContext, PositionedLayoutItem, StyleProperty,
};
use std::collections::HashMap;
use vello::Scene;
use vello::kurbo::{Affine, Rect, RoundedRect};
use vello::peniko::{Color, Fill, FontData};

const WORDS: &[&str] = &[
    "RENDER", "EVERY", "FRAME", "FROM", "TIME", "ZERO", "NEVER", "FROM", "A", "CLOCK", "SEEK",
    "ANYWHERE", "SAME", "PICTURE", "CAPTIONS", "POP", "AND", "FILL", "WORD", "BY", "WORD",
];
pub const WORD_LEN: f64 = 0.4;
pub const PER_PAGE: usize = 4;
const FONT_SIZE: f32 = 110.0;
const BOX_W: f32 = 940.0;

/// What the composition needs from a 2D backend: this is the shape a `Painter` over Vello takes.
pub trait Canvas {
    fn clear(&mut self);
    fn fill_rect(&mut self, rect: Rect, color: Color);
    fn fill_rrect(&mut self, rect: RoundedRect, color: Color);
    fn glyphs(
        &mut self,
        font: &FontData,
        size: f32,
        xf: Affine,
        color: Color,
        glyphs: &[vello::Glyph],
    );
    fn push_clip(&mut self, xf: Affine, rect: Rect);
    fn pop_clip(&mut self);
}

impl Canvas for Scene {
    fn clear(&mut self) {
        self.reset();
    }
    fn fill_rect(&mut self, rect: Rect, color: Color) {
        self.fill(Fill::NonZero, Affine::IDENTITY, color, None, &rect);
    }
    fn fill_rrect(&mut self, rect: RoundedRect, color: Color) {
        self.fill(Fill::NonZero, Affine::IDENTITY, color, None, &rect);
    }
    fn glyphs(
        &mut self,
        font: &FontData,
        size: f32,
        xf: Affine,
        color: Color,
        glyphs: &[vello::Glyph],
    ) {
        self.draw_glyphs(font)
            .font_size(size)
            .transform(xf)
            .brush(color)
            .draw(Fill::NonZero, glyphs.iter().copied());
    }
    fn push_clip(&mut self, xf: Affine, rect: Rect) {
        self.push_clip_layer(Fill::NonZero, xf, &rect);
    }
    fn pop_clip(&mut self) {
        self.pop_layer();
    }
}

pub struct Frames {
    font_cx: FontContext,
    layout_cx: LayoutContext<u32>,
    pub font_name: String,
    pages: HashMap<usize, Layout<u32>>,
    pub relayout: bool,
}

fn page_text(page: usize) -> (String, Vec<std::ops::Range<usize>>) {
    let mut s = String::new();
    let mut spans = vec![];
    for k in 0..PER_PAGE {
        if k > 0 {
            s.push(' ');
        }
        let w = WORDS[(page * PER_PAGE + k) % WORDS.len()];
        spans.push(s.len()..s.len() + w.len());
        s.push_str(w);
    }
    (s, spans)
}

fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
    Color::from_rgba8(r, g, b, a)
}

impl Frames {
    /// Registers only the bundled font file; the system fonts are never consulted by name.
    pub fn new(font_path: &str, relayout: bool) -> anyhow::Result<Self> {
        let mut font_cx = FontContext::new();
        let bytes = std::fs::read(font_path)?;
        let fams = font_cx
            .collection
            .register_fonts(vello::peniko::Blob::from(bytes), None);
        let (fid, _) = fams.first().ok_or_else(|| anyhow::anyhow!("no font"))?;
        let font_name = font_cx
            .collection
            .family_name(*fid)
            .unwrap_or("?")
            .to_string();
        Ok(Self {
            font_cx,
            layout_cx: LayoutContext::new(),
            font_name,
            pages: HashMap::new(),
            relayout,
        })
    }

    fn build(&mut self, page: usize) -> Layout<u32> {
        let (text, spans) = page_text(page);
        let mut b = self
            .layout_cx
            .ranged_builder(&mut self.font_cx, &text, 1.0, true);
        b.push_default(StyleProperty::FontSize(FONT_SIZE));
        b.push_default(StyleProperty::FontFamily(
            FontFamily::named(&self.font_name).into(),
        ));
        b.push_default(StyleProperty::FontWeight(FontWeight::new(600.0)));
        b.push_default(StyleProperty::Brush(u32::MAX));
        for (i, r) in spans.into_iter().enumerate() {
            b.push(StyleProperty::Brush(i as u32), r);
        }
        let mut l: Layout<u32> = b.build(&text);
        l.break_all_lines(Some(BOX_W));
        l.set_alignment(
            parley::Alignment::Center,
            parley::AlignmentOptions::default(),
        );
        l
    }

    fn title<C: Canvas>(&mut self, scene: &mut C) {
        let text = "Crastu render spike";
        let mut b = self
            .layout_cx
            .ranged_builder(&mut self.font_cx, text, 1.0, true);
        b.push_default(StyleProperty::FontSize(64.0));
        b.push_default(StyleProperty::FontFamily(
            FontFamily::named(&self.font_name).into(),
        ));
        b.push_default(StyleProperty::Brush(0));
        let mut l: Layout<u32> = b.build(text);
        l.break_all_lines(Some(900.0));
        for line in l.lines() {
            for item in line.items() {
                if let PositionedLayoutItem::GlyphRun(run) = item {
                    let gl: Vec<vello::Glyph> = run
                        .positioned_glyphs()
                        .map(|g| vello::Glyph {
                            id: g.id,
                            x: g.x + 90.0,
                            y: g.y + 1590.0,
                        })
                        .collect();
                    scene.glyphs(
                        run.run().font(),
                        run.run().font_size(),
                        Affine::IDENTITY,
                        rgba(20, 20, 30, 255),
                        &gl,
                    );
                }
            }
        }
    }

    /// Draws the frame at time `t` seconds into `scene`.
    pub fn draw<C: Canvas>(&mut self, scene: &mut C, t: f64) {
        scene.clear();
        let bg = Rect::new(0.0, 0.0, W as f64, H as f64);
        scene.fill_rect(bg, rgba(14, 16, 22, 255));
        // Static cards, plus one that moves with t (a pure function of t).
        for i in 0..14u32 {
            let x = 60.0 + (i % 2) as f64 * 500.0;
            let dy = if i == 3 { (t * 40.0).sin() * 20.0 } else { 0.0 };
            let y = 80.0 + (i / 2) as f64 * 120.0 + dy;
            let c = rgba(40 + i as u8 * 8, 60, 110, 255);
            let rr = RoundedRect::new(x, y, x + 460.0, y + 100.0, 28.0);
            scene.fill_rrect(rr, c);
        }
        let card = RoundedRect::new(60.0, 1560.0, 1020.0, 1700.0, 40.0);
        scene.fill_rrect(card, rgba(255, 255, 255, 235));
        self.title(scene);

        let page_len = WORD_LEN * PER_PAGE as f64;
        let page = (t / page_len).floor().max(0.0) as usize;
        let layout = if self.relayout {
            self.build(page)
        } else if let Some(l) = self.pages.remove(&page) {
            l
        } else {
            self.build(page)
        };
        let (ox, oy) = ((W as f32 - layout.width()) / 2.0, 1000.0f32);
        let pill = RoundedRect::new(
            ox as f64 - 40.0,
            oy as f64 - 30.0,
            (ox + layout.width()) as f64 + 40.0,
            (oy + layout.height()) as f64 + 30.0,
            48.0,
        );
        scene.fill_rrect(pill, rgba(0, 0, 0, 170));
        let t0 = page as f64 * page_len;
        for line in layout.lines() {
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(run) = item else {
                    continue;
                };
                let idx = run.style().brush;
                if idx == u32::MAX {
                    continue;
                }
                let s = t0 + idx as f64 * WORD_LEN;
                let p = ((t - s) / WORD_LEN).clamp(0.0, 1.0);
                let since = (t - s).max(0.0);
                // Analytic pop: a damped cosine, only once the word has started.
                let pop = if std::env::var_os("NOPOP").is_some() {
                    1.0
                } else if t >= s {
                    1.0 + 0.22 * (-9.0 * since).exp() * (since * 30.0).cos()
                } else {
                    1.0
                };
                let glyphs: Vec<vello::Glyph> = run
                    .positioned_glyphs()
                    .map(|g| vello::Glyph {
                        id: g.id,
                        x: g.x + ox,
                        y: g.y + oy,
                    })
                    .collect();
                let x0 = glyphs.first().map_or(0.0, |g| g.x) as f64;
                let x1 = run
                    .positioned_glyphs()
                    .last()
                    .map_or(0.0, |g| g.x + g.advance + ox) as f64;
                let cy = (run.baseline() + oy) as f64 - FONT_SIZE as f64 * 0.35;
                let cx = (x0 + x1) / 2.0;
                let xf = Affine::translate((cx, cy))
                    * Affine::scale(pop)
                    * Affine::translate((-cx, -cy));
                let font = run.run().font();
                let size = run.run().font_size();
                let draw = |scene: &mut C, col: Color| scene.glyphs(font, size, xf, col, &glyphs);
                if t < s {
                    draw(scene, rgba(150, 150, 160, 255));
                } else if t >= s + WORD_LEN {
                    draw(scene, rgba(255, 255, 255, 255));
                } else {
                    // Karaoke fill: white base, yellow clipped to the played fraction.
                    draw(scene, rgba(255, 255, 255, 255));
                    let clip = Rect::new(x0 - 20.0, cy - 200.0, x0 + (x1 - x0) * p, cy + 200.0);
                    if std::env::var_os("NOCLIP").is_some() {
                        draw(scene, rgba(255, 214, 10, 255));
                    } else {
                        scene.push_clip(xf, clip);
                        draw(scene, rgba(255, 214, 10, 255));
                        scene.pop_clip();
                    }
                }
            }
        }
        if !self.relayout {
            self.pages.insert(page, layout);
        }
    }
}
