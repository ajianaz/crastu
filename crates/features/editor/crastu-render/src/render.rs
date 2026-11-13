//! `render_at(t)`: composition and time in, frame out. No clock, no state between calls.

use crate::caption;
use crate::geom::{Rect, finite};
use crate::model::{Composition, LayerKind, TitleLayer};
use crate::painter::{AssetKind, DisplayList, Painter, TextRun};
use crate::text::TextLayout;

/// A rendered frame: the ordered drawing calls, ready for any [`Painter`] or a GPU slice.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    /// The time it was rendered at, in seconds.
    pub t: f64,
    pub width: u32,
    pub height: u32,
    pub list: DisplayList,
}

/// The time of frame `index` at the composition's frame rate (`0.0` when `fps` is zero).
pub fn frame_time(comp: &Composition, index: u64) -> f64 {
    if comp.fps == 0 {
        return 0.0;
    }
    index as f64 / f64::from(comp.fps)
}

/// Renders `comp` at `t` seconds into a display list.
pub fn render_at(comp: &Composition, t: f64, layout: &dyn TextLayout) -> Frame {
    let mut list = DisplayList::default();
    render_into(comp, t, layout, &mut list);
    Frame {
        t: finite(t),
        width: comp.width,
        height: comp.height,
        list,
    }
}

/// Draws `comp` at `t` seconds through `painter`: layers bottom to top, only those active at `t`.
/// A non-finite `t` is treated as `0.0`.
pub fn render_into<P: Painter>(
    comp: &Composition,
    t: f64,
    layout: &dyn TextLayout,
    painter: &mut P,
) {
    let t = finite(t);
    let frame = (f64::from(comp.width), f64::from(comp.height));
    for layer in &comp.layers {
        if !layer.is_active(t) {
            continue;
        }
        let opacity = finite(layer.opacity).clamp(0.0, 1.0);
        match &layer.kind {
            LayerKind::Solid { color } => {
                painter.draw_rect(Rect::new(0.0, 0.0, frame.0, frame.1), *color, opacity);
            }
            LayerKind::Image { asset, area } => {
                painter.draw_asset(
                    AssetKind::Image,
                    asset,
                    to_pixels(*area, frame),
                    0.0,
                    opacity,
                );
            }
            LayerKind::Video {
                asset,
                area,
                source_start,
            } => {
                let source_time = finite(*source_start) + (t - layer.start);
                painter.draw_asset(
                    AssetKind::Video,
                    asset,
                    to_pixels(*area, frame),
                    source_time,
                    opacity,
                );
            }
            LayerKind::Title(title) => {
                draw_title(title, t - layer.start, opacity, frame, layout, painter)
            }
            LayerKind::Caption(c) => caption::draw(c, t, opacity, frame, layout, painter),
        }
    }
}

pub(crate) fn to_pixels(area: Rect, frame: (f64, f64)) -> Rect {
    Rect::new(
        finite(area.x) * frame.0,
        finite(area.y) * frame.1,
        finite(area.w) * frame.0,
        finite(area.h) * frame.1,
    )
}

fn draw_title<P: Painter>(
    title: &TitleLayer,
    elapsed: f64,
    opacity: f64,
    frame: (f64, f64),
    layout: &dyn TextLayout,
    painter: &mut P,
) {
    let area = to_pixels(title.area, frame);
    let (scale, progress) = match title.enter {
        Some(p) => {
            let (s, p) = p.at(elapsed);
            (s, p.clamp(0.0, 1.0))
        }
        None => (1.0, 1.0),
    };
    let size = finite(title.font_size).max(0.0);
    for (line, rect) in caption::block(layout, &title.text, size, area) {
        painter.draw_text_run(&TextRun {
            text: title.text.get(line.range).unwrap_or_default().to_owned(),
            rect,
            font_size: size,
            color: title.color,
            scale,
            opacity: opacity * progress,
        });
    }
}
