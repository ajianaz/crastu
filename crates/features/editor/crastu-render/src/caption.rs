//! Caption drawing: pages of words with highlight, pop-in and karaoke fill, all derived from the
//! word timings and `t` (no state kept between frames).

use crate::geom::{Rect, finite};
use crate::model::CaptionLayer;
use crate::painter::{Painter, TextRun};
use crate::render::to_pixels;
use crate::text::{Line, TextLayout};

/// Lays `text` out in `area` (frame pixels): wrapped to its width, each line centred, the block
/// centred vertically. Returns each line with its box.
pub(crate) fn block(
    layout: &dyn TextLayout,
    text: &str,
    size: f64,
    area: Rect,
) -> Vec<(Line, Rect)> {
    let size = size.max(0.0);
    let lines = layout.lines(text, size, area.w.max(0.0));
    let lh = layout.measure(" ", size).height;
    let top = area.y + (area.h - lines.len() as f64 * lh) / 2.0;
    lines
        .into_iter()
        .enumerate()
        .map(|(i, line)| {
            let rect = Rect::new(
                area.x + (area.w - line.width) / 2.0,
                top + i as f64 * lh,
                line.width,
                lh,
            );
            (line, rect)
        })
        .collect()
}

/// The page of words shown at `t`: the last page whose first word has started, until its last
/// word ended plus `linger`.
fn page_at(
    words: &[crate::text::WordTiming],
    per: usize,
    linger: f64,
    t: f64,
) -> Option<&[crate::text::WordTiming]> {
    let per = per.max(1);
    let mut found = None;
    for (i, first) in words.iter().step_by(per).enumerate() {
        if finite(first.start) <= t {
            found = Some(i);
        } else {
            break;
        }
    }
    let i = found?;
    let page = &words[i * per..words.len().min((i + 1) * per)];
    let end = page.iter().map(|w| finite(w.end)).fold(f64::MIN, f64::max);
    (t < end + finite(linger).max(0.0)).then_some(page)
}

pub(crate) fn draw<P: Painter>(
    layer: &CaptionLayer,
    t: f64,
    opacity: f64,
    frame: (f64, f64),
    layout: &dyn TextLayout,
    painter: &mut P,
) {
    let st = &layer.style;
    let Some(page) = page_at(&layer.words, st.words_per_page, st.linger, t) else {
        return;
    };
    // The page as one string, with the byte span of each word in it.
    let mut text = String::new();
    let mut spans = Vec::with_capacity(page.len());
    for w in page {
        let word = w.text.trim();
        if word.is_empty() {
            continue;
        }
        if !text.is_empty() {
            text.push(' ');
        }
        spans.push((text.len(), text.len() + word.len(), w));
        text.push_str(word);
    }
    let size = finite(st.font_size).max(0.0);
    let lines = block(layout, &text, size, to_pixels(st.area, frame));
    let unspoken = finite(st.unspoken_opacity).clamp(0.0, 1.0);
    for (start, end, w) in spans {
        let Some((line, line_rect)) = lines
            .iter()
            .find(|(l, _)| l.range.start <= start && start < l.range.end)
        else {
            continue;
        };
        let x = line_rect.x
            + layout
                .measure(text.get(line.range.start..start).unwrap_or_default(), size)
                .width;
        let word = text.get(start..end).unwrap_or_default();
        let rect = Rect::new(
            x,
            line_rect.y,
            layout.measure(word, size).width,
            line_rect.h,
        );

        let (ws, we) = (finite(w.start), finite(w.end));
        let active = t >= ws && t < we;
        let spoken = t >= we;
        let scale = st.pop_in.map_or(1.0, |p| p.at(t - ws).0);
        let word_opacity = opacity * if t < ws { unspoken } else { 1.0 };
        let mut color = st.color;
        if active && let Some(h) = st.highlight {
            color = h.color;
        }
        let mut run = TextRun {
            text: word.to_owned(),
            rect,
            font_size: size,
            color,
            scale,
            opacity: word_opacity,
        };
        match st.karaoke {
            Some(k) if spoken => {
                run.color = k.fill;
                painter.draw_text_run(&run);
            }
            Some(k) if active => {
                painter.draw_text_run(&run);
                let shown = rect.scaled_about_centre(scale);
                let progress = ((t - ws) / (we - ws)).clamp(0.0, 1.0);
                painter.push_clip(Rect::new(shown.x, shown.y, shown.w * progress, shown.h));
                run.color = k.fill;
                painter.draw_text_run(&run);
                painter.pop_clip();
            }
            _ => painter.draw_text_run(&run),
        }
    }
}
