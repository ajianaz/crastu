use std::rc::Rc;

use crastu_ui::tokens::{Tokens, layout};
use gpui::{
    App, ElementId, FocusHandle, FontWeight, IntoElement, MouseButton, RenderOnce, SharedString,
    Window, div, prelude::*, px,
};

use super::common::{Callback, dim, edge, ring};
use crastu_controls::choice::{Choice, ChoiceInput};
use crastu_controls::look::{self, ToggleState};
use crastu_controls::nav::Move;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChoiceKind {
    /// A compact group of options in mono type, the chosen one on a raised chip.
    Segmented,
    /// A row of tabs with an accent line under the chosen one.
    Tabs,
}

/// A row of choices that is one tab stop: arrows, Home and End move the choice.
#[derive(IntoElement)]
pub struct Choices {
    id: ElementId,
    kind: ChoiceKind,
    labels: Vec<SharedString>,
    choice: Choice,
    tokens: Tokens,
    show_focus: bool,
    focus: Option<FocusHandle>,
    on_select: Option<Callback<ChoiceInput>>,
}

impl Choices {
    pub fn new(
        id: impl Into<ElementId>,
        kind: ChoiceKind,
        labels: &[&str],
        choice: &Choice,
        tokens: Tokens,
    ) -> Choices {
        Choices {
            id: id.into(),
            kind,
            labels: labels
                .iter()
                .map(|l| SharedString::from(l.to_string()))
                .collect(),
            choice: choice.clone(),
            tokens,
            show_focus: false,
            focus: None,
            on_select: None,
        }
    }

    pub fn show_focus(mut self, f: bool) -> Choices {
        self.show_focus = f;
        self
    }

    pub fn track_focus(mut self, h: &FocusHandle) -> Choices {
        self.focus = Some(h.clone());
        self
    }

    pub fn on_select(
        mut self,
        f: impl Fn(ChoiceInput, &mut Window, &mut App) + 'static,
    ) -> Choices {
        self.on_select = Some(Rc::new(f));
        self
    }
}

impl RenderOnce for Choices {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        let t = self.tokens;
        let c = t.colors;
        let group_focus =
            self.show_focus || self.focus.as_ref().is_some_and(|h| h.is_focused(window));
        let mut row = div().id(self.id).flex().items_center();
        row = match self.kind {
            ChoiceKind::Segmented => row
                .gap(px(2.0))
                .p(px(2.0))
                .rounded(px(layout::RADIUS_CONTROL))
                .bg(c.surface.rgb())
                .border_1()
                .border_color(c.border.rgb()),
            ChoiceKind::Tabs => row.gap(px(4.0)).border_b_1().border_color(c.border.rgb()),
        };
        for (i, label) in self.labels.iter().enumerate() {
            let state = ToggleState {
                on: i == self.choice.selected,
                focus: group_focus && i == self.choice.selected,
                disabled: !self.choice.enabled.get(i).copied().unwrap_or(false),
            };
            let look = match self.kind {
                ChoiceKind::Segmented => look::segment(state, c),
                ChoiceKind::Tabs => look::tab(state, c),
            };
            let mut item = div()
                .id(("choice", i))
                .flex()
                .items_center()
                .px(px(12.0))
                .h(px(if self.kind == ChoiceKind::Tabs {
                    t.hit_target
                } else {
                    t.hit_target - 6.0
                }))
                .rounded(px(4.0))
                .text_size(px(13.0))
                .text_color(look.fg.rgb())
                .child(label.clone());
            item = match self.kind {
                ChoiceKind::Segmented => item.font_family(t.fonts.mono),
                ChoiceKind::Tabs => item
                    .font_family(t.fonts.ui)
                    .font_weight(if state.on {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::NORMAL
                    })
                    .rounded_none()
                    .border_b_2()
                    .border_color(edge(look.border)),
            };
            if let Some(bg) = look.bg {
                item = item.bg(bg.rgb());
            }
            if look.dim {
                item = item.opacity(dim());
            }
            if let Some(r) = look.ring {
                item = item.shadow(ring(r, c.bg));
            }
            if !state.disabled {
                item = item.cursor_pointer();
                if let Some(f) = self.on_select.clone() {
                    item =
                        item.on_click(move |_, window, cx| f(ChoiceInput::Select(i), window, cx));
                }
            }
            row = row.child(item);
        }
        if let Some(h) = &self.focus {
            let (on_select, click_handle) = (self.on_select, h.clone());
            row = row
                .track_focus(h)
                .tab_stop(true)
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    window.focus(&click_handle, cx)
                })
                .on_key_down(move |ev, window, cx| {
                    let key = ev.keystroke.key.as_str();
                    if Move::from_key(key, true).is_some() {
                        if let Some(f) = &on_select {
                            f(ChoiceInput::Key(key.to_string()), window, cx);
                        }
                        cx.stop_propagation();
                    }
                });
        }
        row
    }
}
