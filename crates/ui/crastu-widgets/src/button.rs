use crastu_ui::tokens::{Tokens, layout};
use gpui::{
    App, ClickEvent, ElementId, FocusHandle, FontWeight, IntoElement, RenderOnce, SharedString,
    Window, div, prelude::*, px,
};

use super::common::{Spinner, dim, edge, ring};
use crastu_controls::look::{self, State, Variant};

type Handler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// A button. `state` forces one of the documented states (for the gallery and tests of the
/// look); without it the button reacts to the pointer and the keyboard by itself.
#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    label: SharedString,
    variant: Variant,
    tokens: Tokens,
    forced: Option<State>,
    disabled: bool,
    loading: bool,
    focus: Option<FocusHandle>,
    on_click: Option<Handler>,
}

impl Button {
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        variant: Variant,
        tokens: Tokens,
    ) -> Button {
        Button {
            id: id.into(),
            label: label.into(),
            variant,
            tokens,
            forced: None,
            disabled: false,
            loading: false,
            focus: None,
            on_click: None,
        }
    }

    pub fn primary(id: impl Into<ElementId>, label: impl Into<SharedString>, t: Tokens) -> Button {
        Button::new(id, label, Variant::Primary, t)
    }

    pub fn secondary(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        t: Tokens,
    ) -> Button {
        Button::new(id, label, Variant::Secondary, t)
    }

    pub fn ghost(id: impl Into<ElementId>, label: impl Into<SharedString>, t: Tokens) -> Button {
        Button::new(id, label, Variant::Ghost, t)
    }

    pub fn danger(id: impl Into<ElementId>, label: impl Into<SharedString>, t: Tokens) -> Button {
        Button::new(id, label, Variant::Danger, t)
    }

    pub fn state(mut self, state: State) -> Button {
        self.forced = Some(state);
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Button {
        self.disabled = disabled;
        self
    }

    pub fn loading(mut self, loading: bool) -> Button {
        self.loading = loading;
        self
    }

    pub fn track_focus(mut self, handle: &FocusHandle) -> Button {
        self.focus = Some(handle.clone());
        self
    }

    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Button {
        self.on_click = Some(Box::new(f));
        self
    }
}

impl RenderOnce for Button {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let c = self.tokens.colors;
        let resting = if self.disabled {
            State::Disabled
        } else if self.loading {
            State::Loading
        } else {
            State::Default
        };
        let shown = self.forced.unwrap_or(resting);
        let live = self.forced.is_none() && shown == State::Default;
        let look = look::button(self.variant, shown, c);
        let hover = look::button(self.variant, State::Hover, c);
        let pressed = look::button(self.variant, State::Pressed, c);
        let focused = look::button(self.variant, State::Focus, c);

        let mut el = div()
            .id(self.id)
            .flex()
            .items_center()
            .justify_center()
            .gap(px(6.0))
            .h(px(self.tokens.hit_target))
            .px(px(16.0))
            .rounded(px(layout::RADIUS_CONTROL))
            .border_1()
            .border_color(edge(look.border))
            .font_family(self.tokens.fonts.ui)
            .text_size(px(13.0))
            .font_weight(FontWeight::MEDIUM)
            .text_color(look.fg.rgb());
        if let Some(bg) = look.bg {
            el = el.bg(bg.rgb());
        }
        if look.dim {
            el = el.opacity(dim());
        }
        if let Some(r) = look.ring {
            el = el.shadow(ring(r, c.bg));
        }
        if live {
            let (hover_bg, pressed_bg) = (hover.bg, pressed.bg);
            let ring_color = focused.ring.unwrap_or(c.accent);
            el = el
                .cursor_pointer()
                .hover(move |s| match hover_bg {
                    Some(b) => s.bg(b.rgb()),
                    None => s,
                })
                .active(move |s| match pressed_bg {
                    Some(b) => s.bg(b.rgb()),
                    None => s,
                })
                .focus_visible(move |s| s.shadow(ring(ring_color, c.bg)));
            if let Some(handle) = &self.focus {
                el = el.track_focus(handle).tab_stop(true);
            }
            if let Some(f) = self.on_click {
                el = el.on_click(f);
            }
        }
        if shown == State::Loading {
            el = el.child(Spinner::new(look.fg, &self.tokens));
        }
        el.child(self.label)
    }
}
