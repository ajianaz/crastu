//! The AI and feedback page: badges, shortcuts and tooltip, toasts and the approval dialog.

use crastu_controls::{Badge, DialogEffect, DialogInput, ToastKind};
use crastu_ui::tokens::Tokens;
use crastu_widgets::{
    BadgeView, Button, Checkbox, DialogView, Kbd, ToastView, label_text, tooltip,
};
use gpui::{App, Context, ParentElement, Window, div, prelude::*, px};

use crate::app::{Gallery, caption, heading, hook, hook0};

/// What a dialog input does to the gallery, and where the focus goes afterwards.
fn dialog_input(cx: &Context<Gallery>) -> impl Fn(DialogInput, &mut Window, &mut App) + 'static {
    let this = cx.entity().downgrade();
    move |input, window, app| {
        let Some(this) = this.upgrade() else { return };
        let (effect, dialog, opener) = this.update(app, |g, cx| {
            let effect = g.dialog.apply(&input);
            cx.notify();
            (effect, g.dialog_focus.clone(), g.opener_focus.clone())
        });
        match effect {
            DialogEffect::FocusDialog => window.focus(&dialog, app),
            DialogEffect::ReturnFocus => window.focus(&opener, app),
            DialogEffect::None => {}
        }
    }
}

pub fn feedback_page(g: &Gallery, t: Tokens, cx: &mut Context<Gallery>) -> gpui::Div {
    let mut badges = div().flex().flex_wrap().gap(px(8.0));
    for kind in Badge::ALL {
        badges = badges.child(BadgeView::new(kind, t));
    }

    let kbd = |keys: &'static str, what: &'static str| {
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(Kbd::new(keys, t))
            .child(caption(&t, what))
    };
    let split_tip = div()
        .id("split-tip")
        .tooltip(tooltip("Split clip at playhead", Some("\u{2318}B"), t))
        .child(Button::secondary("split", "Split", t));

    let mut toasts = div().flex().flex_col().gap(px(8.0)).w(px(640.0));
    for (i, toast) in g.toasts.items().iter().enumerate() {
        let mut view = ToastView::new(toast, t)
            .on_action(hook(cx, |g, id, cx| {
                g.toasts.dismiss(id);
                cx.notify();
            }))
            .on_dismiss(hook(cx, |g, id, cx| {
                g.toasts.dismiss(id);
                cx.notify();
            }));
        if let Some(h) = g.toast_focus.get(i) {
            view = view.track_focus(h);
        }
        toasts = toasts.child(view);
    }
    let again = Button::ghost("toast-again", "Show another toast", t).on_click({
        let more = hook0(cx, |g, cx| {
            let n = g.toasts.items().len();
            g.toasts.push(
                ToastKind::Success,
                &format!("Saved {n} changes"),
                Some("Undo"),
            );
            cx.notify();
        });
        move |_, w, a| more(w, a)
    });

    let open = dialog_input(cx);
    let approve = Button::primary("open-dialog", "Review an agent's changes", t)
        .track_focus(&g.opener_focus)
        .on_click(move |_, window, app| open(DialogInput::Open, window, app));

    div()
        .flex()
        .flex_col()
        .items_start()
        .gap(px(20.0))
        .child(heading(&t, "Licence and source badges"))
        .child(badges)
        .child(heading(&t, "Shortcuts and tooltip"))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(24.0))
                .child(kbd("\u{2318}K", "Search"))
                .child(kbd("\u{2325}Space", "Dictate"))
                .child(kbd("Space", "Play"))
                .child(kbd("\u{21E7}\u{2318}E", "Export"))
                .child(split_tip),
        )
        .child(heading(&t, "Toasts"))
        .child(toasts)
        .child(again)
        .child(heading(&t, "Approving an external agent's action"))
        .child(label_text(
            "Opens a dialog: Escape or a click outside closes it and the focus returns here.",
            t.colors.text_2,
            &t,
        ))
        .child(approve)
}

/// The dialog, drawn over the whole window while it is open.
pub fn dialog_overlay(g: &Gallery, t: Tokens, cx: &mut Context<Gallery>) -> Option<DialogView> {
    if !g.dialog.open {
        return None;
    }
    let c = t.colors;
    let change = |mark: &'static str, text: &'static str, time: &'static str| {
        div()
            .flex()
            .items_center()
            .gap(px(12.0))
            .h(px(32.0))
            .px(px(12.0))
            .child(label_text(mark, c.accent, &t))
            .child(div().flex_1().child(label_text(text, c.text, &t)))
            .child(label_text(time, c.text_3, &t).font_family(t.fonts.mono))
    };
    let list = div()
        .flex()
        .flex_col()
        .rounded(px(6.0))
        .border_1()
        .border_color(c.border.rgb())
        .child(change(
            "+",
            "Add clip \"Six months of savings\" to Video",
            "00:00",
        ))
        .child(change("~", "Trim 6 seconds from the start", "00:06"))
        .child(change("+", "Add captions in the Bold style", "00:00"))
        .child(change("\u{2212}", "Remove old captions", "00:12"));
    let (decline, approve) = (dialog_input(cx), dialog_input(cx));
    Some(
        DialogView::new("Claude wants to change the timeline", t)
            .subtitle("Project Money Talk Podcast #12, via MCP")
            .track_focus(&g.dialog_focus)
            .on_input(dialog_input(cx))
            .body(list)
            .body(
                Checkbox::new(
                    "dont-ask",
                    "Don't ask again for changes from Claude",
                    g.dont_ask,
                    t,
                )
                .track_focus(&g.dont_ask_focus)
                .on_toggle(hook0(cx, |g, cx| {
                    g.dont_ask = !g.dont_ask;
                    cx.notify();
                })),
            )
            .action(
                Button::secondary("decline", "Decline", t)
                    .on_click(move |_, w, a| decline(DialogInput::Close, w, a)),
            )
            .action(
                Button::primary("approve", "Approve 4 changes", t)
                    .on_click(move |_, w, a| approve(DialogInput::Close, w, a)),
            ),
    )
}
