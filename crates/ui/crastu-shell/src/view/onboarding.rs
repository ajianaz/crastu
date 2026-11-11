//! The onboarding screens: a card with the step list on the left and the current step on the
//! right. It draws a `crastu_app::Onboarding` and sends `OnboardingAction`s; what Continue may do
//! and which steps can be reached is decided in `crastu-app`.

use crastu_app::{Action, Onboarding, OnboardingAction, Permission, Step};
use crastu_ui::tokens::{Tokens, layout};
use crastu_widgets::{Button, label_text};
use gpui::{Context, ElementId, FontWeight, IntoElement, div, prelude::*, px};

use super::onboarding_steps::{agents, done, models, shortcut, welcome};
use super::{Dispatch, Shell, caption};
use crate::steps::{self, PermissionButton, StepMark};

fn send(
    d: &Dispatch,
    a: OnboardingAction,
) -> impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static {
    let d = d.clone();
    move |_, _, cx| d(Action::Onboarding(a.clone()), cx)
}

fn heading(text: &str, about: &str, t: &Tokens) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(
            label_text(text.to_string(), t.colors.text, t)
                .text_size(px(24.0))
                .font_weight(FontWeight::SEMIBOLD),
        )
        .child(label_text(about.to_string(), t.colors.text_2, t))
}

/// The step list on the left.
fn step_list(shell: &Shell, ob: &Onboarding, t: Tokens, d: &Dispatch) -> gpui::Div {
    let c = t.colors;
    let mut col = div().flex().flex_col().gap(px(4.0));
    for (i, row) in steps::step_rows(ob).into_iter().enumerate() {
        let (circle_bg, circle_fg, mark) = match row.mark {
            StepMark::Done => (c.accent_soft, c.accent, "\u{2713}".to_string()),
            StepMark::Current => (c.accent, c.on_accent, row.number.to_string()),
            StepMark::Upcoming => (c.raised, c.text_2, row.number.to_string()),
        };
        let mut item = div()
            .id(ElementId::NamedInteger("ob-step".into(), i as u64))
            .flex()
            .items_center()
            .gap(px(12.0))
            .h(px(36.0))
            .px(px(10.0))
            .rounded(px(layout::RADIUS_CONTROL))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(22.0))
                    .rounded_full()
                    .bg(circle_bg.rgb())
                    .text_size(px(11.0))
                    .text_color(circle_fg.rgb())
                    .child(mark),
            )
            .child(label_text(
                row.step.title(),
                if row.mark == StepMark::Current {
                    c.text
                } else {
                    c.text_2
                },
                &t,
            ));
        if row.mark == StepMark::Current {
            item = item.bg(c.raised.rgb()).font_weight(FontWeight::MEDIUM);
        }
        if row.reachable && row.mark != StepMark::Current {
            let d = d.clone();
            let step = row.step;
            let ring = c.accent;
            item = item
                .cursor_pointer()
                .track_focus(&shell.focus.steps[i])
                .tab_stop(true)
                .focus_visible(move |s| s.border_1().border_color(ring.rgb()))
                .on_click(move |_, _, cx| d(Action::Onboarding(OnboardingAction::GoTo(step)), cx));
        } else if !row.reachable {
            item = item.opacity(0.45);
        }
        col = col.child(item);
    }
    col
}

fn permissions(shell: &Shell, ob: &Onboarding, t: Tokens, d: &Dispatch) -> gpui::Div {
    let c = t.colors;
    let mut list = div()
        .flex()
        .flex_col()
        .rounded(px(layout::RADIUS_CARD))
        .border_1()
        .border_color(c.border.rgb());
    for (i, p) in Permission::ALL.into_iter().enumerate() {
        let state = ob.permission(p);
        let id = ElementId::NamedInteger("ob-perm".into(), i as u64);
        let ask = send(d, OnboardingAction::AskPermission(p));
        let action = match steps::permission_button(state) {
            PermissionButton::Allowed => {
                label_text("\u{2713}  Allowed", c.accent, &t).into_any_element()
            }
            PermissionButton::Allow => Button::secondary(id, "Allow", t)
                .track_focus(&shell.focus.perms[i])
                .on_click(ask)
                .into_any_element(),
            PermissionButton::OpenSystemSettings => Button::primary(id, "Open System Settings", t)
                .track_focus(&shell.focus.perms[i])
                .on_click(ask)
                .into_any_element(),
        };
        let mut name = div().flex().gap(px(8.0)).items_baseline().child(
            label_text(steps::permission_name(p), c.text, &t).font_weight(FontWeight::MEDIUM),
        );
        if p.optional() {
            name = name.child(caption("optional", &t));
        }
        let mut row = div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(16.0))
            .px(px(16.0))
            .py(px(12.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(name)
                    .child(caption(p.why(), &t)),
            )
            .child(action);
        if i > 0 {
            row = row.border_t_1().border_color(c.border.rgb());
        }
        list = list.child(row);
    }
    list
}

/// The whole onboarding screen.
pub fn screen(
    shell: &Shell,
    ob: &Onboarding,
    t: Tokens,
    cx: &mut Context<Shell>,
) -> gpui::Stateful<gpui::Div> {
    let c = t.colors;
    let d = Shell::dispatcher(cx);
    let (title, about) = match ob.step {
        Step::Welcome => (
            "Welcome to Crastu",
            "Your voice and video studio. It runs locally, works offline, and your AI agents can use it too.",
        ),
        Step::Permissions => (
            "Give Crastu what it needs",
            "macOS asks for each one. You can change them later in System Settings, Privacy and Security.",
        ),
        Step::Models => (
            "Download your first models",
            "Picked for Indonesian and English on this Mac. Downloads continue in the background.",
        ),
        Step::Shortcut => (
            "Pick your dictation shortcut",
            "Choose the keys you press to talk.",
        ),
        Step::Agents => (
            "Let your AI agents use Crastu",
            "Crastu speaks MCP. Agents such as Claude Code can find moments, cut clips and edit timelines for you, within the limits you set.",
        ),
        Step::Done => ("You're set", "Everything is ready."),
    };
    let body = match ob.step {
        Step::Welcome => welcome(t),
        Step::Permissions => permissions(shell, ob, t, &d),
        Step::Models => models(shell, ob, t, &d),
        Step::Shortcut => shortcut(shell, ob, t, &Shell::shortcut_chooser(cx)),
        Step::Agents => agents(shell, t),
        Step::Done => done(ob, t),
    };

    let first = ob.step == Step::Welcome;
    let last = ob.step == Step::Done;
    let mut buttons = div().flex().gap(px(8.0));
    if !first {
        buttons = buttons.child(
            Button::secondary("ob-back", "Back", t)
                .track_focus(&shell.focus.back)
                .on_click(send(&d, OnboardingAction::Back)),
        );
    }
    buttons = buttons.child(if last {
        Button::primary("ob-finish", "Open Crastu", t)
            .track_focus(&shell.focus.next)
            .on_click(send(&d, OnboardingAction::Finish))
    } else {
        Button::primary("ob-next", "Continue", t)
            .track_focus(&shell.focus.next)
            .disabled(!ob.can_continue())
            .on_click(send(&d, OnboardingAction::Next))
    });

    let skip = {
        let this = cx.entity().downgrade();
        Button::ghost("ob-skip", "Skip setup for now", t)
            .track_focus(&shell.focus.skip)
            .on_click(move |_, _, app| {
                this.update(app, |s, cx| {
                    s.skipped_setup = true;
                    s.check_focus = true;
                    cx.notify();
                })
                .ok();
            })
    };

    let sidebar = div()
        .flex()
        .flex_col()
        .justify_between()
        .w(px(250.0))
        .flex_none()
        .p(px(16.0))
        .border_r_1()
        .border_color(c.border.rgb())
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(16.0))
                .child(
                    label_text("Set up Crastu", c.text, &t)
                        .text_size(px(16.0))
                        .font_weight(FontWeight::SEMIBOLD),
                )
                .child(step_list(shell, ob, t, &d)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .items_start()
                .gap(px(8.0))
                .child(caption(
                    "Everything runs on this Mac. No account needed.",
                    &t,
                ))
                .child(skip),
        );

    let main = div()
        .flex()
        .flex_col()
        .justify_between()
        .flex_1()
        .min_w_0()
        .p(px(32.0))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(24.0))
                .child(heading(title, about, &t))
                .child(body),
        )
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(caption(steps::progress(ob), &t))
                .child(buttons),
        );

    div()
        .id("onboarding")
        .track_focus(&shell.focus.root)
        .flex()
        .items_center()
        .justify_center()
        .size_full()
        .bg(c.bg.rgb())
        .child(
            div()
                .flex()
                .w(px(1040.0))
                .max_w_full()
                .h(px(680.0))
                .max_h_full()
                .rounded(px(12.0))
                .border_1()
                .border_color(c.border.rgb())
                .bg(c.surface.rgb())
                .child(sidebar)
                .child(main),
        )
}
