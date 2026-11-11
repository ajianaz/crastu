use crastu_controls::badge::{self, Badge};
use crastu_ui::tokens::Tokens;
use gpui::{App, IntoElement, RenderOnce, SharedString, Window, div, prelude::*, px};

use super::common::edge;

/// A licence or status pill.
#[derive(IntoElement)]
pub struct BadgeView {
    kind: Badge,
    tokens: Tokens,
}

impl BadgeView {
    pub fn new(kind: Badge, tokens: Tokens) -> BadgeView {
        BadgeView { kind, tokens }
    }
}

impl RenderOnce for BadgeView {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let look = badge::badge(self.kind, self.tokens.colors);
        let mut el = div()
            .flex()
            .items_center()
            .h(px(22.0))
            .px(px(10.0))
            .rounded_full()
            .border_1()
            .border_color(edge(Some(look.edge)))
            .font_family(self.tokens.fonts.ui)
            .text_size(px(12.0))
            .text_color(look.fg.rgb());
        if let Some(fill) = look.fill {
            el = el.bg(fill.rgb());
        }
        el.child(self.kind.label())
    }
}

/// A keyboard shortcut chip, for example `⌘K`.
#[derive(IntoElement)]
pub struct Kbd {
    keys: SharedString,
    tokens: Tokens,
}

impl Kbd {
    pub fn new(keys: impl Into<SharedString>, tokens: Tokens) -> Kbd {
        Kbd {
            keys: keys.into(),
            tokens,
        }
    }
}

impl RenderOnce for Kbd {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let c = self.tokens.colors;
        div()
            .flex()
            .items_center()
            .h(px(22.0))
            .px(px(6.0))
            .rounded(px(4.0))
            .border_1()
            .border_color(c.border.rgb())
            .bg(c.surface.rgb())
            .font_family(self.tokens.fonts.mono)
            .text_size(px(11.0))
            .text_color(c.text_2.rgb())
            .child(self.keys)
    }
}
