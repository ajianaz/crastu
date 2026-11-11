use std::rc::Rc;

use crastu_ui::tokens::Tokens;
use gpui::{
    App, ElementId, FocusHandle, IntoElement, RenderOnce, SharedString, Window, div, prelude::*, px,
};

use super::common::{Callback, dim, label_text, ring};
use crastu_controls::look::{self, ToggleState};

/// A click asks for a flip; the owner flips its own current value, so two clicks between
/// frames both count.
type Toggle = Callback<()>;

macro_rules! toggle_builders {
    ($ty:ident) => {
        impl $ty {
            pub fn disabled(mut self, d: bool) -> $ty {
                self.disabled = d;
                self
            }

            /// Shows the focus ring without real focus (the gallery).
            pub fn show_focus(mut self, f: bool) -> $ty {
                self.show_focus = f;
                self
            }

            pub fn track_focus(mut self, h: &FocusHandle) -> $ty {
                self.focus = Some(h.clone());
                self
            }

            pub fn on_toggle(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> $ty {
                self.on_toggle = Some(Rc::new(move |(), window, cx| f(window, cx)));
                self
            }
        }
    };
}

/// Wraps the box or track and its label into one clickable, focusable row.
fn row(
    id: ElementId,
    disabled: bool,
    focus: Option<FocusHandle>,
    on_toggle: Option<Toggle>,
    ring_color: Option<crastu_ui::tokens::Color>,
    t: &Tokens,
) -> gpui::Stateful<gpui::Div> {
    let c = t.colors;
    let mut r = div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(8.0))
        .rounded(px(4.0));
    if disabled {
        return r.opacity(dim());
    }
    r = r
        .cursor_pointer()
        .focus_visible(move |s| s.shadow(ring(c.accent, c.bg)));
    if let Some(color) = ring_color {
        r = r.shadow(ring(color, c.bg));
    }
    if let Some(h) = &focus {
        r = r.track_focus(h).tab_stop(true);
    }
    if let Some(f) = on_toggle {
        r = r.on_click(move |_, window, cx| f((), window, cx));
    }
    r
}

#[derive(IntoElement)]
pub struct Checkbox {
    id: ElementId,
    label: SharedString,
    checked: bool,
    tokens: Tokens,
    disabled: bool,
    show_focus: bool,
    focus: Option<FocusHandle>,
    on_toggle: Option<Toggle>,
}

impl Checkbox {
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        checked: bool,
        tokens: Tokens,
    ) -> Checkbox {
        Checkbox {
            id: id.into(),
            label: label.into(),
            checked,
            tokens,
            disabled: false,
            show_focus: false,
            focus: None,
            on_toggle: None,
        }
    }
}

toggle_builders!(Checkbox);

impl RenderOnce for Checkbox {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let t = self.tokens;
        let c = t.colors;
        let look = look::checkbox(
            ToggleState {
                on: self.checked,
                focus: self.show_focus,
                disabled: self.disabled,
            },
            c,
        );
        let mut boxed = div()
            .flex()
            .items_center()
            .justify_center()
            .size(px(16.0))
            .rounded(px(4.0))
            .border_1()
            .border_color(look.edge.rgb())
            .text_size(px(11.0))
            .text_color(look.mark.rgb());
        if let Some(fill) = look.fill {
            boxed = boxed.bg(fill.rgb()).child("\u{2713}");
        }
        row(
            self.id,
            self.disabled,
            self.focus,
            self.on_toggle,
            look.ring,
            &t,
        )
        .child(boxed)
        .child(label_text(self.label, c.text, &t))
    }
}

#[derive(IntoElement)]
pub struct Switch {
    id: ElementId,
    label: SharedString,
    on: bool,
    tokens: Tokens,
    disabled: bool,
    show_focus: bool,
    focus: Option<FocusHandle>,
    on_toggle: Option<Toggle>,
}

impl Switch {
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        on: bool,
        tokens: Tokens,
    ) -> Switch {
        Switch {
            id: id.into(),
            label: label.into(),
            on,
            tokens,
            disabled: false,
            show_focus: false,
            focus: None,
            on_toggle: None,
        }
    }
}

toggle_builders!(Switch);

impl RenderOnce for Switch {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let t = self.tokens;
        let c = t.colors;
        let look = look::switch(
            ToggleState {
                on: self.on,
                focus: self.show_focus,
                disabled: self.disabled,
            },
            c,
        );
        let mut track = div()
            .flex()
            .items_center()
            .w(px(34.0))
            .h(px(20.0))
            .px(px(3.0))
            .rounded_full()
            .border_1()
            .border_color(look.edge.rgb());
        if let Some(fill) = look.fill {
            track = track.bg(fill.rgb());
        }
        if self.on {
            track = track.justify_end();
        }
        let thumb = div().size(px(12.0)).rounded_full().bg(look.mark.rgb());
        row(
            self.id,
            self.disabled,
            self.focus,
            self.on_toggle,
            look.ring,
            &t,
        )
        .child(track.child(thumb))
        .child(label_text(self.label, c.text, &t))
    }
}
