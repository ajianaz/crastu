use std::rc::Rc;

use crastu_ui::tokens::{Tokens, layout};
use gpui::{
    App, ElementId, FocusHandle, IntoElement, MouseButton, RenderOnce, SharedString, Window,
    deferred, div, prelude::*, px,
};

use super::common::{Callback, label_text, ring};
use crastu_controls::choice::SelectModel;
use crastu_controls::look::{self, FieldState};

/// A select: a trigger that shows the chosen option, and the open list under it. The owner keeps
/// the [`SelectModel`] and stores what `on_change` hands back (the list opens, moves and closes
/// through it too).
#[derive(IntoElement)]
pub struct SelectView {
    id: ElementId,
    prefix: SharedString,
    options: Vec<SharedString>,
    model: SelectModel,
    tokens: Tokens,
    show_focus: bool,
    focus: Option<FocusHandle>,
    on_change: Option<Callback<SelectModel>>,
}

impl SelectView {
    pub fn new(
        id: impl Into<ElementId>,
        prefix: impl Into<SharedString>,
        options: &[&str],
        model: &SelectModel,
        tokens: Tokens,
    ) -> SelectView {
        SelectView {
            id: id.into(),
            prefix: prefix.into(),
            options: options
                .iter()
                .map(|o| SharedString::from(o.to_string()))
                .collect(),
            model: model.clone(),
            tokens,
            show_focus: false,
            focus: None,
            on_change: None,
        }
    }

    pub fn show_focus(mut self, f: bool) -> SelectView {
        self.show_focus = f;
        self
    }

    pub fn track_focus(mut self, h: &FocusHandle) -> SelectView {
        self.focus = Some(h.clone());
        self
    }

    pub fn on_change(
        mut self,
        f: impl Fn(SelectModel, &mut Window, &mut App) + 'static,
    ) -> SelectView {
        self.on_change = Some(Rc::new(f));
        self
    }
}

impl RenderOnce for SelectView {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        let t = self.tokens;
        let c = t.colors;
        let focused = self.show_focus
            || self.model.open
            || self.focus.as_ref().is_some_and(|h| h.is_focused(window));
        let look = look::field(
            if focused {
                FieldState::Focus
            } else {
                FieldState::Default
            },
            c,
        );
        let model = self.model.clone();

        let mut trigger = div()
            .id(self.id)
            .flex()
            .items_center()
            .gap(px(8.0))
            .h(px(t.hit_target))
            .px(px(10.0))
            .rounded(px(layout::RADIUS_CONTROL))
            .border_1()
            .border_color(look.border.rgb())
            .bg(look.bg.rgb())
            .cursor_pointer()
            .child(label_text(self.prefix, c.text_2, &t))
            .child(
                div().flex_1().child(label_text(
                    self.options
                        .get(model.selected)
                        .cloned()
                        .unwrap_or_default(),
                    c.text,
                    &t,
                )),
            )
            .child(label_text(
                if model.open { "\u{02C4}" } else { "\u{02C5}" },
                c.text_2,
                &t,
            ));
        if let Some(r) = look.ring {
            trigger = trigger.shadow(ring(r, c.bg));
        }
        if let Some(cb) = self.on_change.clone() {
            let m = model.clone();
            trigger = trigger.on_click(move |_, window, cx| {
                let mut next = m.clone();
                if next.open {
                    next.close()
                } else {
                    next.open()
                };
                cb(next, window, cx);
            });
        }
        if let Some(h) = &self.focus {
            let (m, cb, click_handle) = (model.clone(), self.on_change.clone(), h.clone());
            trigger = trigger
                .track_focus(h)
                .tab_stop(true)
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    window.focus(&click_handle, cx)
                })
                .on_key_down(move |ev, window, cx| {
                    let mut next = m.clone();
                    let key = ev.keystroke.key.as_str();
                    next.key(key);
                    if next != m {
                        if let Some(cb) = &cb {
                            cb(next, window, cx);
                        }
                        cx.stop_propagation();
                    }
                });
        }

        let mut outer = div().relative().child(trigger);
        if model.open {
            let mut list = div()
                .absolute()
                .top(px(t.hit_target + 4.0))
                .left_0()
                .w_full()
                .p(px(4.0))
                .rounded(px(layout::RADIUS_CONTROL))
                .border_1()
                .border_color(c.border.rgb())
                .bg(c.surface.rgb())
                .shadow_md();
            // The list shows what both the options and the model know about.
            for (i, option) in self.options.iter().enumerate().take(model.enabled.len()) {
                let row_look = look::option_row(i == model.selected, i == model.highlighted, c);
                let mut row = div()
                    .id(("option", i))
                    .flex()
                    .items_center()
                    .justify_between()
                    .h(px(28.0))
                    .px(px(8.0))
                    .rounded(px(4.0))
                    .cursor_pointer()
                    .child(label_text(option.clone(), row_look.fg, &t))
                    .when(i == model.selected, |r| {
                        r.child(label_text("\u{2713}", row_look.fg, &t))
                    });
                if let Some(bg) = row_look.bg {
                    row = row.bg(bg.rgb());
                }
                if let Some(cb) = self.on_change.clone() {
                    let m = model.clone();
                    row = row.on_click(move |_, window, cx| {
                        let mut next = m.clone();
                        next.choose(i);
                        cb(next, window, cx);
                    });
                }
                list = list.child(row);
            }
            outer = outer.child(deferred(list).with_priority(1));
        }
        outer
    }
}
