use crastu_controls::dialog::DialogInput;
use crastu_ui::tokens::{Tokens, layout};
use gpui::{
    AnyElement, App, FocusHandle, FontWeight, IntoElement, MouseButton, RenderOnce, SharedString,
    Window, div, prelude::*, px,
};

use super::common::{Callback, hsla, label_text};

/// A modal dialog over the whole window: a dimmed scrim and a panel with a title, a body and a
/// row of actions. Escape and a click on the scrim send `DialogInput`; the owner applies it to
/// its `DialogModel` and moves the focus as the returned effect says (into the dialog on open,
/// back to what opened it on close). Tab is not trapped inside the panel yet.
#[derive(IntoElement)]
pub struct DialogView {
    title: SharedString,
    subtitle: Option<SharedString>,
    tokens: Tokens,
    body: Vec<AnyElement>,
    actions: Vec<AnyElement>,
    focus: Option<FocusHandle>,
    on_input: Option<Callback<DialogInput>>,
}

impl DialogView {
    pub fn new(title: impl Into<SharedString>, tokens: Tokens) -> DialogView {
        DialogView {
            title: title.into(),
            subtitle: None,
            tokens,
            body: Vec::new(),
            actions: Vec::new(),
            focus: None,
            on_input: None,
        }
    }

    pub fn subtitle(mut self, s: impl Into<SharedString>) -> DialogView {
        self.subtitle = Some(s.into());
        self
    }

    pub fn body(mut self, e: impl IntoElement) -> DialogView {
        self.body.push(e.into_any_element());
        self
    }

    pub fn action(mut self, e: impl IntoElement) -> DialogView {
        self.actions.push(e.into_any_element());
        self
    }

    pub fn track_focus(mut self, h: &FocusHandle) -> DialogView {
        self.focus = Some(h.clone());
        self
    }

    pub fn on_input(
        mut self,
        f: impl Fn(DialogInput, &mut Window, &mut App) + 'static,
    ) -> DialogView {
        self.on_input = Some(std::rc::Rc::new(f));
        self
    }
}

impl RenderOnce for DialogView {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let t = self.tokens;
        let c = t.colors;
        let mut panel = div()
            .id("dialog")
            .occlude()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .w(px(480.0))
            .p(px(20.0))
            .rounded(px(layout::RADIUS_CARD))
            .border_1()
            .border_color(c.border.rgb())
            .bg(c.surface.rgb())
            .shadow_lg()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        label_text(self.title, c.text, &t)
                            .text_size(px(16.0))
                            .font_weight(FontWeight::SEMIBOLD),
                    )
                    .children(self.subtitle.map(|s| label_text(s, c.text_2, &t))),
            )
            .children(self.body)
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .children(self.actions),
            );
        let mut scrim = div()
            .id("dialog-scrim")
            .occlude()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(hsla(c.bg).opacity(0.72));
        if let Some(f) = self.on_input.clone() {
            scrim = scrim.on_mouse_down(MouseButton::Left, move |_, window, cx| {
                f(DialogInput::Close, window, cx)
            });
            // A press on the panel itself must not reach the scrim.
            panel = panel.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        }
        if let Some(h) = &self.focus {
            let f = self.on_input;
            panel = panel.track_focus(h).on_key_down(move |ev, window, cx| {
                if ev.keystroke.key == "escape" {
                    if let Some(f) = &f {
                        f(DialogInput::Key("escape".into()), window, cx);
                    }
                    cx.stop_propagation();
                }
            });
        }
        scrim.child(panel)
    }
}
