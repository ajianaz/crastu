//! Helpers shared by the views.

use std::time::Duration;

use crastu_ui::tokens::{Color, Tokens};
use gpui::{
    Animation, AnimationExt as _, BoxShadow, Div, FontWeight, Hsla, IntoElement, SharedString,
    Styled, div, point, prelude::*, px,
};

use crastu_controls::look::DIM_PERCENT;

/// A change callback: the new value, then the window and app.
pub(crate) type Callback<V> = std::rc::Rc<dyn Fn(V, &mut gpui::Window, &mut gpui::App) + 'static>;

pub(crate) fn hsla(c: Color) -> Hsla {
    c.rgb().into()
}

/// A border colour, or no visible border (it keeps its width so nothing shifts).
pub(crate) fn edge(c: Option<Color>) -> Hsla {
    c.map_or_else(gpui::transparent_black, hsla)
}

pub(crate) fn dim() -> f32 {
    DIM_PERCENT as f32 / 100.0
}

/// The focus ring: a gap in the page colour, then two pixels of the ring colour.
pub(crate) fn ring(ring: Color, gap: Color) -> Vec<BoxShadow> {
    let shadow = |color: Color, spread: f32| BoxShadow {
        color: hsla(color),
        offset: point(px(0.0), px(0.0)),
        blur_radius: px(0.0),
        spread_radius: px(spread),
        inset: false,
    };
    vec![shadow(ring, 4.0), shadow(gap, 2.0)]
}

/// A 13 px body label in the UI family.
pub fn label_text(text: impl Into<SharedString>, color: Color, t: &Tokens) -> Div {
    div()
        .font_family(t.fonts.ui)
        .text_size(px(13.0))
        .font_weight(FontWeight::NORMAL)
        .text_color(color.rgb())
        .child(text.into())
}

/// A braille-dot spinner, still when the user asks for reduced motion.
#[derive(IntoElement)]
pub struct Spinner {
    color: Color,
    reduce_motion: bool,
}

const FRAMES: [&str; 10] = [
    "\u{280B}", "\u{2819}", "\u{2839}", "\u{2838}", "\u{283C}", "\u{2834}", "\u{2826}", "\u{2827}",
    "\u{2807}", "\u{280F}",
];

impl Spinner {
    pub fn new(color: Color, t: &Tokens) -> Spinner {
        Spinner {
            color,
            reduce_motion: t.reduce_motion,
        }
    }
}

impl RenderOnce for Spinner {
    fn render(self, _: &mut gpui::Window, _: &mut gpui::App) -> impl IntoElement {
        let color = self.color.rgb();
        let frame = move |i: usize| {
            div()
                .w(px(14.0))
                .text_size(px(14.0))
                .text_color(color)
                .child(FRAMES[i % FRAMES.len()])
        };
        if self.reduce_motion {
            return frame(0).into_any_element();
        }
        frame(0)
            .with_animation(
                "spinner",
                Animation::new(Duration::from_millis(800)).repeat(),
                move |_, t| frame(((t * FRAMES.len() as f32) as usize).min(FRAMES.len() - 1)),
            )
            .into_any_element()
    }
}

use gpui::RenderOnce;
