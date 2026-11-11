use crastu_controls::toast::Toast;
use crastu_ui::tokens::Tokens;
use gpui::{
    App, ElementId, FocusHandle, FontWeight, IntoElement, RenderOnce, Window, div, prelude::*, px,
};

use super::common::{Callback, ring};

/// One toast: a mark, the message and an optional action. With a focus handle it can be
/// reached with Tab, dismissed with Escape or Delete and its action run with Enter or Space.
#[derive(IntoElement)]
pub struct ToastView {
    toast: Toast,
    tokens: Tokens,
    focus: Option<FocusHandle>,
    on_action: Option<Callback<u64>>,
    on_dismiss: Option<Callback<u64>>,
}

impl ToastView {
    pub fn new(toast: &Toast, tokens: Tokens) -> ToastView {
        ToastView {
            toast: toast.clone(),
            tokens,
            focus: None,
            on_action: None,
            on_dismiss: None,
        }
    }

    pub fn track_focus(mut self, h: &FocusHandle) -> ToastView {
        self.focus = Some(h.clone());
        self
    }

    pub fn on_action(mut self, f: impl Fn(u64, &mut Window, &mut App) + 'static) -> ToastView {
        self.on_action = Some(std::rc::Rc::new(f));
        self
    }

    pub fn on_dismiss(mut self, f: impl Fn(u64, &mut Window, &mut App) + 'static) -> ToastView {
        self.on_dismiss = Some(std::rc::Rc::new(f));
        self
    }
}

impl RenderOnce for ToastView {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let t = self.tokens;
        let c = t.colors;
        let id = self.toast.id;
        let mut el = div()
            .id(ElementId::NamedInteger("toast".into(), id))
            .flex()
            .items_center()
            .gap(px(12.0))
            .h(px(48.0))
            .px(px(16.0))
            .rounded(px(10.0))
            .border_1()
            .border_color(c.border.rgb())
            .bg(c.surface.rgb())
            .font_family(t.fonts.ui)
            .text_size(px(13.0))
            .text_color(c.text.rgb())
            .child(
                div()
                    .text_color(self.toast.kind.color(c).rgb())
                    .child(self.toast.kind.mark()),
            )
            .child(div().flex_1().child(self.toast.message.clone()));
        if let Some(label) = &self.toast.action {
            let mut action = div()
                .id(ElementId::NamedInteger("toast-action".into(), id))
                .cursor_pointer()
                .font_weight(FontWeight::MEDIUM)
                .text_color(c.accent.rgb())
                .child(label.clone());
            if let Some(f) = &self.on_action {
                let f = f.clone();
                action = action.on_click(move |_, window, cx| f(id, window, cx));
            }
            el = el.child(action);
        }
        if let Some(h) = &self.focus {
            let (on_dismiss, on_action) = (self.on_dismiss, self.on_action);
            el = el
                .track_focus(h)
                .tab_stop(true)
                .focus_visible(move |s| s.shadow(ring(c.accent, c.bg)))
                .on_key_down(move |ev, window, cx| {
                    match ev.keystroke.key.as_str() {
                        "escape" | "delete" | "backspace" => {
                            if let Some(f) = &on_dismiss {
                                f(id, window, cx);
                            }
                            cx.stop_propagation();
                        }
                        // The action (Show, Details, Retry) from the keyboard.
                        "enter" | "space" => {
                            if let Some(f) = &on_action {
                                f(id, window, cx);
                                cx.stop_propagation();
                            }
                        }
                        _ => {}
                    }
                });
        }
        el
    }
}
