use std::cell::Cell;
use std::rc::Rc;

use crastu_ui::tokens::Tokens;
use gpui::{
    App, Bounds, ElementId, FocusHandle, FontWeight, IntoElement, MouseButton, Pixels, RenderOnce,
    SharedString, Window, canvas, div, prelude::*, px, relative,
};

use super::common::{Callback, dim, label_text, ring};
use crastu_controls::value::{Nudge, RangeSlider, Slider, Thumb};

const THUMB: f32 = 14.0;

fn track(t: &Tokens, from: f32, to: f32, bounds: &Rc<Cell<Bounds<Pixels>>>) -> gpui::Div {
    let c = t.colors;
    let store = bounds.clone();
    div()
        .relative()
        .h(px(20.0))
        .w_full()
        .child(
            canvas(move |b, _, _| store.set(b), |_, _, _, _| {})
                .absolute()
                .size_full(),
        )
        .child(
            div()
                .absolute()
                .top(px(8.0))
                .h(px(4.0))
                .w_full()
                .rounded_full()
                .bg(c.border.rgb()),
        )
        .child(
            div()
                .absolute()
                .top(px(8.0))
                .h(px(4.0))
                .left(relative(from))
                .w(relative((to - from).max(0.0)))
                .rounded_full()
                .bg(c.accent.rgb()),
        )
}

fn thumb(t: &Tokens, at: f32, hollow: bool, focused: bool) -> gpui::Div {
    let c = t.colors;
    let mut d = div()
        .absolute()
        .top(px(3.0))
        .left(relative(at))
        .ml(px(-THUMB / 2.0))
        .size(px(THUMB))
        .rounded_full()
        .border_2()
        .border_color(c.accent.rgb())
        .bg(if hollow { c.bg.rgb() } else { c.accent.rgb() });
    if focused {
        d = d.shadow(ring(c.accent, c.bg));
    }
    d
}

fn head(t: &Tokens, label: SharedString, value: SharedString) -> gpui::Div {
    div()
        .flex()
        .justify_between()
        .child(label_text(label, t.colors.text, t).font_weight(FontWeight::NORMAL))
        .child(label_text(value, t.colors.text_2, t).font_family(t.fonts.mono))
}

fn along(x: Pixels, bounds: &Bounds<Pixels>) -> f32 {
    let w = f32::from(bounds.size.width);
    if w <= 0.0 {
        0.0
    } else {
        f32::from(x - bounds.origin.x) / w
    }
}

/// A slider with a label and a value read-out (for example the voiceover volume).
#[derive(IntoElement)]
pub struct SliderView {
    id: ElementId,
    label: SharedString,
    value_text: SharedString,
    slider: Slider,
    tokens: Tokens,
    disabled: bool,
    show_focus: bool,
    focus: Option<FocusHandle>,
    on_change: Option<Callback<Slider>>,
}

impl SliderView {
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        value_text: impl Into<SharedString>,
        slider: Slider,
        tokens: Tokens,
    ) -> SliderView {
        SliderView {
            id: id.into(),
            label: label.into(),
            value_text: value_text.into(),
            slider,
            tokens,
            disabled: false,
            show_focus: false,
            focus: None,
            on_change: None,
        }
    }

    pub fn disabled(mut self, d: bool) -> SliderView {
        self.disabled = d;
        self
    }

    pub fn show_focus(mut self, f: bool) -> SliderView {
        self.show_focus = f;
        self
    }

    pub fn track_focus(mut self, h: &FocusHandle) -> SliderView {
        self.focus = Some(h.clone());
        self
    }

    pub fn on_change(mut self, f: impl Fn(Slider, &mut Window, &mut App) + 'static) -> SliderView {
        self.on_change = Some(Rc::new(f));
        self
    }
}

impl RenderOnce for SliderView {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        let t = self.tokens;
        let focused = self.show_focus || self.focus.as_ref().is_some_and(|h| h.is_focused(window));
        let bounds = Rc::new(Cell::new(Bounds::default()));
        // The pointer handlers sit on the track, not on the label row above it.
        let mut trk = track(&t, 0.0, self.slider.fraction(), &bounds).child(thumb(
            &t,
            self.slider.fraction(),
            false,
            focused,
        ));
        let mut body = div().id(self.id).flex().flex_col().gap(px(4.0)).child(head(
            &t,
            self.label,
            self.value_text,
        ));
        if self.disabled {
            return body.child(trk).opacity(dim());
        }
        let (slider, on_change) = (self.slider, self.on_change);
        let set = {
            let on_change = on_change.clone();
            move |mut s: Slider, f: f32, window: &mut Window, cx: &mut App| {
                s.set_fraction(f);
                if let Some(cb) = &on_change {
                    cb(s, window, cx);
                }
            }
        };
        let (down_bounds, move_bounds) = (bounds.clone(), bounds);
        let (set_down, set_move) = (set.clone(), set);
        if let Some(h) = &self.focus {
            let key_handle = h.clone();
            body =
                body.track_focus(&key_handle)
                    .tab_stop(true)
                    .on_key_down(move |ev, window, cx| {
                        if let Some(n) = Nudge::from_key(&ev.keystroke.key) {
                            let mut s = slider;
                            s.nudge(n);
                            if let Some(cb) = &on_change {
                                cb(s, window, cx);
                            }
                            cx.stop_propagation();
                        }
                    });
        }
        let down_handle = self.focus.clone();
        trk = trk.on_mouse_down(MouseButton::Left, move |ev, window, cx| {
            if let Some(h) = &down_handle {
                window.focus(h, cx);
            }
            set_down(slider, along(ev.position.x, &down_bounds.get()), window, cx);
        });
        body.child(trk.on_mouse_move(move |ev, window, cx| {
            if ev.pressed_button == Some(MouseButton::Left) {
                set_move(slider, along(ev.position.x, &move_bounds.get()), window, cx);
            }
        }))
    }
}

/// A range slider with two thumbs (for example the clip length).
#[derive(IntoElement)]
pub struct RangeSliderView {
    id: ElementId,
    label: SharedString,
    value_text: SharedString,
    range: RangeSlider,
    tokens: Tokens,
    focus: Option<(FocusHandle, FocusHandle)>,
    on_change: Option<Callback<RangeSlider>>,
}

impl RangeSliderView {
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        value_text: impl Into<SharedString>,
        range: RangeSlider,
        tokens: Tokens,
    ) -> RangeSliderView {
        RangeSliderView {
            id: id.into(),
            label: label.into(),
            value_text: value_text.into(),
            range,
            tokens,
            focus: None,
            on_change: None,
        }
    }

    /// One focus handle per thumb, so each can be reached with Tab and moved with the arrows.
    pub fn track_focus(mut self, low: &FocusHandle, high: &FocusHandle) -> RangeSliderView {
        self.focus = Some((low.clone(), high.clone()));
        self
    }

    pub fn on_change(
        mut self,
        f: impl Fn(RangeSlider, &mut Window, &mut App) + 'static,
    ) -> RangeSliderView {
        self.on_change = Some(Rc::new(f));
        self
    }
}

impl RenderOnce for RangeSliderView {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        let t = self.tokens;
        let r = self.range;
        let bounds = Rc::new(Cell::new(Bounds::default()));
        let focused = |h: Option<&FocusHandle>| h.is_some_and(|h| h.is_focused(window));
        let (low_h, high_h) = match &self.focus {
            Some((l, h)) => (Some(l), Some(h)),
            None => (None, None),
        };
        let on_change = self.on_change;
        // Each thumb is its own focusable element, so Tab reaches it and the arrows move it.
        let make = |which: Thumb, n: usize, handle: Option<&FocusHandle>| {
            let mut el = thumb(&t, r.fraction(which), true, focused(handle)).id(("thumb", n));
            if let Some(h) = handle {
                let cb = on_change.clone();
                el = el
                    .track_focus(h)
                    .tab_stop(true)
                    .on_key_down(move |ev, window, cx| {
                        if let Some(nudge) = Nudge::from_key(&ev.keystroke.key) {
                            let mut next = r;
                            next.nudge(which, nudge);
                            if let Some(cb) = &cb {
                                cb(next, window, cx);
                            }
                            cx.stop_propagation();
                        }
                    });
            }
            el
        };
        let mut trk = track(&t, r.fraction(Thumb::Low), r.fraction(Thumb::High), &bounds)
            .child(make(Thumb::Low, 0, low_h))
            .child(make(Thumb::High, 1, high_h));
        let body = div().id(self.id).flex().flex_col().gap(px(4.0)).child(head(
            &t,
            self.label,
            self.value_text,
        ));
        let drag_handles = self.focus.clone();
        if let Some((low, high)) = self.focus {
            let (down_bounds, down_cb) = (bounds.clone(), on_change.clone());
            trk = trk.on_mouse_down(MouseButton::Left, move |ev, window, cx| {
                let f = along(ev.position.x, &down_bounds.get());
                let thumb = r.nearest(f);
                window.focus(if thumb == Thumb::Low { &low } else { &high }, cx);
                let mut next = r;
                next.set_fraction(thumb, f);
                if let Some(cb) = &down_cb {
                    cb(next, window, cx);
                }
            });
        }
        // The press focused the nearer thumb; a drag keeps moving that one.
        body.child(trk.on_mouse_move(move |ev, window, cx| {
            if ev.pressed_button == Some(MouseButton::Left) {
                let f = along(ev.position.x, &bounds.get());
                let thumb = match &drag_handles {
                    Some((low, _)) if low.is_focused(window) => Thumb::Low,
                    Some(_) => Thumb::High,
                    None => r.nearest(f),
                };
                let mut next = r;
                next.set_fraction(thumb, f);
                if let Some(cb) = &on_change {
                    cb(next, window, cx);
                }
            }
        }))
    }
}
