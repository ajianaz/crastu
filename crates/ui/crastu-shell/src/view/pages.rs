//! The content area: Settings (Appearance works), placeholders for the modules whose screens
//! are not built yet, the onboarding stub and the toast stack.

use crastu_app::{Action, ModuleId, Nav, SettingsCategory};
use crastu_controls::Choice;
use crastu_ui::tokens::Tokens;
use crastu_widgets::{ChoiceKind, Choices, Switch, ToastView, label_text};
use gpui::{Context, FontWeight, div, prelude::*, px};

use super::{Shell, caption};
use crate::appearance::{self, Setting};

pub(super) fn page(t: Tokens) -> gpui::Stateful<gpui::Div> {
    div()
        .id("content")
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .overflow_y_scroll()
        .p(px(24.0))
        .gap(px(20.0))
        .bg(t.colors.bg.rgb())
}

pub(super) fn title(text: &str, t: &Tokens) -> gpui::Div {
    label_text(text.to_string(), t.colors.text, t)
        .text_size(px(24.0))
        .font_weight(FontWeight::SEMIBOLD)
}

pub fn content(
    shell: &Shell,
    nav: &Nav,
    t: Tokens,
    cx: &mut Context<Shell>,
) -> gpui::Stateful<gpui::Div> {
    match (nav.active, nav.settings_category) {
        (ModuleId::Settings, SettingsCategory::Appearance) => appearance_page(shell, t, cx),
        (ModuleId::Settings, SettingsCategory::Privacy) => privacy_page(shell, t, cx),
        (ModuleId::Settings, SettingsCategory::Agents) => shell.agents_page(t, cx),
        (ModuleId::Settings, category) => placeholder(category.title(), t),
        (module, _) => placeholder(module.title(), t),
    }
}

fn placeholder(name: &str, t: Tokens) -> gpui::Stateful<gpui::Div> {
    page(t)
        .child(title(name, &t))
        .child(caption("This screen is built in its own slice. The shell, rail and Settings > Appearance are working.", &t))
}

fn appearance_page(shell: &Shell, t: Tokens, cx: &mut Context<Shell>) -> gpui::Stateful<gpui::Div> {
    let appearance = shell.model.state().appearance;
    let pick = Shell::chooser(cx);
    let row =
        |id: &'static str, name: &str, about: &str, setting: Setting, focus: &gpui::FocusHandle| {
            let labels = appearance::labels(setting);
            let choice = Choice::new(labels.len(), appearance::selected(setting, &appearance));
            let pick = pick.clone();
            div()
                .flex()
                .flex_col()
                .items_start()
                .gap(px(6.0))
                .child(
                    label_text(name.to_string(), t.colors.text, &t).font_weight(FontWeight::MEDIUM),
                )
                .child(caption(about.to_string(), &t))
                .child(
                    Choices::new(id, ChoiceKind::Segmented, &labels, &choice, t)
                        .track_focus(focus)
                        .on_select(move |input, _, app| pick(setting, input, app)),
                )
        };
    page(t)
        .child(title("Appearance", &t))
        .child(row("theme", "Theme", "Colours only; nothing else changes between themes.", Setting::Theme, &shell.focus.theme))
        .child(row("font", "Font pairing", "Interface and monospace fonts. A family that is not installed falls back to a system font.", Setting::Font, &shell.focus.font))
        .child(row("density", "Density", "Compact controls are 28 px high, comfortable ones 36 px.", Setting::Density, &shell.focus.density))
        .child(row("motion", "Motion", "Reduce removes animations. Follow system uses the operating system setting.", Setting::ReduceMotion, &shell.focus.motion))
}

/// The Offline switch. A click only asks for a flip (`Action::ToggleOffline`); the model flips the
/// value it holds when the action arrives, so the page keeps no copy of it.
fn privacy_page(shell: &Shell, t: Tokens, cx: &mut Context<Shell>) -> gpui::Stateful<gpui::Div> {
    let network = shell.model.state().network;
    let send = Shell::dispatcher(cx);
    // A disabled switch draws dimmed and never calls back.
    let switch = Switch::new("offline", "Offline", network.offline, t)
        .track_focus(&shell.focus.offline)
        .disabled(network.locked_by_env)
        .on_toggle(move |_, app| send(Action::ToggleOffline, app));
    let mut section = div()
        .flex()
        .flex_col()
        .items_start()
        .gap(px(6.0))
        .child(switch)
        .child(caption(
            "Refuses every network request, including model downloads. Installed models keep working.",
            &t,
        ));
    if network.locked_by_env {
        section = section.child(caption(
            "Set by the CRASTU_OFFLINE environment variable.",
            &t,
        ));
    }
    page(t).child(title("Privacy", &t)).child(section)
}

/// Errors and notices at the bottom right.
pub fn toast_stack(shell: &Shell, t: Tokens, cx: &mut Context<Shell>) -> gpui::Div {
    let mut stack = div()
        .absolute()
        .bottom(px(16.0))
        .right(px(16.0))
        .w(px(380.0))
        .flex()
        .flex_col()
        .gap(px(8.0));
    for toast in shell.toasts.items() {
        let send = cx.entity().downgrade();
        let id = toast.id;
        stack = stack.child(ToastView::new(toast, t).on_dismiss(move |_, _, app| {
            send.update(app, |s, cx| {
                s.toasts.dismiss(id);
                cx.notify();
            })
            .ok();
        }));
    }
    stack
}
