use crastu_ui::tokens::{Tokens, layout};
use gpui::{AnyView, App, Context, IntoElement, Render, SharedString, Window, div, prelude::*, px};

/// The content of a tooltip: the text and, when there is one, its shortcut. Drawn light on a
/// dark theme and dark on a light one (the text colour as the fill).
pub struct TooltipView {
    text: SharedString,
    shortcut: Option<SharedString>,
    tokens: Tokens,
}

impl Render for TooltipView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let c = self.tokens.colors;
        let mut el = div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .h(px(28.0))
            .px(px(10.0))
            .rounded(px(layout::RADIUS_CONTROL))
            .bg(c.text.rgb())
            .font_family(self.tokens.fonts.ui)
            .text_size(px(12.0))
            .text_color(c.bg.rgb())
            .child(self.text.clone());
        if let Some(s) = &self.shortcut {
            el = el.child(
                div()
                    .font_family(self.tokens.fonts.mono)
                    .text_color(c.text_3.rgb())
                    .child(s.clone()),
            );
        }
        el
    }
}

/// For `.tooltip(...)` on any interactive element.
pub fn tooltip(
    text: impl Into<SharedString>,
    shortcut: Option<&str>,
    tokens: Tokens,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let (text, shortcut) = (
        text.into(),
        shortcut.map(|s| SharedString::from(s.to_string())),
    );
    move |_, cx| {
        let (text, shortcut) = (text.clone(), shortcut.clone());
        cx.new(|_| TooltipView {
            text,
            shortcut,
            tokens,
        })
        .into()
    }
}
