use std::rc::Rc;

use crastu_ui::tokens::{Tokens, layout};
use gpui::{
    App, ElementId, FocusHandle, FontWeight, IntoElement, KeyDownEvent, MouseButton, RenderOnce,
    SharedString, Window, div, prelude::*, px,
};

use super::common::{dim, label_text, ring};
use crastu_controls::look::{self, FieldState};
use crastu_controls::text::TextModel;

type Change = Rc<dyn Fn(TextModel, &mut Window, &mut App) + 'static>;

/// A single-line text field with a label and help text. The owner keeps the [`TextModel`] and
/// stores what `on_change` hands back. `state` can force Error or Disabled; Focus follows the
/// focus handle, or is forced with `state(FieldState::Focus)` for the gallery.
#[derive(IntoElement)]
pub struct TextField {
    id: ElementId,
    label: SharedString,
    model: TextModel,
    placeholder: SharedString,
    help: Option<SharedString>,
    state: FieldState,
    tokens: Tokens,
    focus: Option<FocusHandle>,
    on_change: Option<Change>,
}

impl TextField {
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        model: &TextModel,
        tokens: Tokens,
    ) -> TextField {
        TextField {
            id: id.into(),
            label: label.into(),
            model: model.clone(),
            placeholder: SharedString::default(),
            help: None,
            state: FieldState::Default,
            tokens,
            focus: None,
            on_change: None,
        }
    }

    pub fn placeholder(mut self, text: impl Into<SharedString>) -> TextField {
        self.placeholder = text.into();
        self
    }

    pub fn help(mut self, text: impl Into<SharedString>) -> TextField {
        self.help = Some(text.into());
        self
    }

    pub fn state(mut self, state: FieldState) -> TextField {
        self.state = state;
        self
    }

    pub fn track_focus(mut self, handle: &FocusHandle) -> TextField {
        self.focus = Some(handle.clone());
        self
    }

    pub fn on_change(
        mut self,
        f: impl Fn(TextModel, &mut Window, &mut App) + 'static,
    ) -> TextField {
        self.on_change = Some(Rc::new(f));
        self
    }
}

/// Applies one key press to the text. None when the key is not for the field.
fn apply(model: &TextModel, ev: &KeyDownEvent, cx: &App) -> Option<TextModel> {
    let ks = &ev.keystroke;
    let mut m = model.clone();
    if ks.modifiers.platform || ks.modifiers.control {
        if ks.key == "v" {
            m.insert(&cx.read_from_clipboard()?.text()?);
            return Some(m);
        }
        return None;
    }
    match ks.key.as_str() {
        "backspace" => m.backspace(),
        "delete" => m.delete(),
        "left" => m.left(),
        "right" => m.right(),
        "home" => m.home(),
        "end" => m.end(),
        _ => m.insert(ks.key_char.as_deref()?),
    }
    // A key that changed nothing (Enter, Tab, an arrow at the end) is left for others to handle.
    (m != *model).then_some(m)
}

impl RenderOnce for TextField {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        let c = self.tokens.colors;
        let focused = self.focus.as_ref().is_some_and(|h| h.is_focused(window));
        let state = match self.state {
            FieldState::Default if focused => FieldState::Focus,
            s => s,
        };
        let look = look::field(state, c);
        let editable = state != FieldState::Disabled;
        let chars: Vec<char> = self.model.text().chars().collect();
        let (before, after): (String, String) = (
            chars[..self.model.cursor()].iter().collect(),
            chars[self.model.cursor()..].iter().collect(),
        );

        let mut input = div()
            .id(self.id)
            .flex()
            .items_center()
            .h(px(self.tokens.hit_target))
            .px(px(10.0))
            .rounded(px(layout::RADIUS_CONTROL))
            .border_1()
            .border_color(look.border.rgb())
            .bg(look.bg.rgb())
            .font_family(self.tokens.fonts.ui)
            .text_size(px(13.0))
            .text_color(look.fg.rgb());
        if look.dim {
            input = input.opacity(dim());
        }
        if let Some(r) = look.ring {
            input = input.shadow(ring(r, c.bg));
        }
        if self.model.is_empty() && !focused {
            input = input.child(
                div()
                    .text_color(look.placeholder.rgb())
                    .child(self.placeholder),
            );
        } else {
            input = input.child(before);
            if focused || state == FieldState::Focus {
                input = input.child(div().w(px(1.0)).h(px(16.0)).bg(c.accent.rgb()));
            }
            input = input.child(after);
        }
        if let (true, Some(handle)) = (editable, &self.focus) {
            let (model, on_change, mouse_handle) = (self.model, self.on_change, handle.clone());
            input = input
                .track_focus(handle)
                .tab_stop(true)
                .cursor_text()
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    window.focus(&mouse_handle, cx)
                })
                .on_key_down(move |ev, window, cx| {
                    if let (Some(next), Some(f)) = (apply(&model, ev, cx), &on_change) {
                        f(next, window, cx);
                        cx.stop_propagation();
                    }
                });
        }

        let mut col = div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .font_family(self.tokens.fonts.ui)
                    .text_size(px(12.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(c.text.rgb())
                    .child(self.label),
            )
            .child(input);
        if let Some(help) = self.help {
            col = col.child(label_text(help, look.help, &self.tokens).text_size(px(12.0)));
        }
        col
    }
}
