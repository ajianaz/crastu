//! The bodies of the onboarding steps that are not the permission list.

use crastu_app::{Action, Onboarding, OnboardingAction};
use crastu_controls::Choice;
use crastu_ui::tokens::{Tokens, layout};
use crastu_widgets::{BadgeView, Button, Checkbox, ChoiceKind, Choices, Kbd, label_text};
use gpui::{ClipboardItem, FontWeight, SharedString, div, prelude::*, px};

use super::{Dispatch, Shell, ShortcutPick, caption};
use crate::steps::{self, ShortcutChoice};

pub(super) fn models(shell: &Shell, ob: &Onboarding, t: Tokens, d: &Dispatch) -> gpui::Div {
    let c = t.colors;
    let cell = |w: f32| div().w(px(w)).flex_none();
    let head = div()
        .flex()
        .items_center()
        .px(px(16.0))
        .h(px(32.0))
        .child(div().flex_1().child(caption("Model", &t)))
        .child(cell(190.0).child(caption("Used for", &t)))
        .child(cell(70.0).child(caption("Size", &t)))
        .child(cell(130.0).child(caption("Licence", &t)));
    let mut table = div()
        .flex()
        .flex_col()
        .rounded(px(layout::RADIUS_CARD))
        .border_1()
        .border_color(c.border.rgb())
        .child(head);
    for (i, m) in steps::model_rows(ob).into_iter().enumerate() {
        let id = m.id.clone();
        let d2 = d.clone();
        let mut check = Checkbox::new(
            SharedString::from(format!("ob-model-{i}")),
            m.name,
            m.selected,
            t,
        )
        .disabled(m.required)
        .on_toggle(move |_, cx| {
            d2(
                Action::Onboarding(OnboardingAction::ToggleModel(id.clone())),
                cx,
            )
        });
        if let (false, Some(h)) = (m.required, shell.focus.models.get(i)) {
            check = check.track_focus(h);
        }
        table = table.child(
            div()
                .flex()
                .items_center()
                .px(px(16.0))
                .py(px(10.0))
                .border_t_1()
                .border_color(c.border.rgb())
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .items_baseline()
                        .gap(px(8.0))
                        .child(check)
                        .child(caption(m.note, &t)),
                )
                .child(cell(190.0).child(label_text(m.used_for, c.text_2, &t)))
                .child(cell(70.0).child(label_text(m.size, c.text_3, &t).font_family(t.fonts.mono)))
                .child(cell(130.0).flex().child(BadgeView::new(m.licence, t))),
        );
    }
    div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(table)
        .child(caption(
            "Saved to ~/Library/Application Support/Crastu/models",
            &t,
        ))
}

pub(super) fn shortcut(
    shell: &Shell,
    ob: &Onboarding,
    t: Tokens,
    pick: &ShortcutPick,
) -> gpui::Div {
    let preset_labels: Vec<String> = steps::SHORTCUT_PRESETS
        .iter()
        .map(|p| steps::shortcut_label(p))
        .collect();
    let preset_refs: Vec<&str> = preset_labels.iter().map(String::as_str).collect();
    let preset = Choice::new(
        preset_refs.len(),
        steps::choice_selected(ob, ShortcutChoice::Preset),
    );
    let hold = Choice::new(2, steps::choice_selected(ob, ShortcutChoice::HoldMode));
    let (pick_preset, pick_hold) = (pick.clone(), pick.clone());
    div()
        .flex()
        .flex_col()
        .items_start()
        .gap(px(16.0))
        .child(Kbd::new(steps::shortcut_label(&ob.shortcut), t))
        .child(
            Choices::new("ob-preset", ChoiceKind::Segmented, &preset_refs, &preset, t)
                .track_focus(&shell.focus.preset)
                .on_select(move |input, _, cx| pick_preset(ShortcutChoice::Preset, input, cx)),
        )
        .child(
            Choices::new(
                "ob-hold",
                ChoiceKind::Segmented,
                &steps::HOLD_LABELS,
                &hold,
                t,
            )
            .track_focus(&shell.focus.hold)
            .on_select(move |input, _, cx| pick_hold(ShortcutChoice::HoldMode, input, cx)),
        )
        .child(caption(
            "Custom keys, and the Fn key on Apple keyboards, come with Settings > Shortcuts.",
            &t,
        ))
}

pub(super) fn agents(shell: &Shell, t: Tokens) -> gpui::Div {
    let c = t.colors;
    let command = div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .px(px(14.0))
        .h(px(44.0))
        .rounded(px(layout::RADIUS_CONTROL))
        .border_1()
        .border_color(c.border.rgb())
        .bg(c.surface.rgb())
        .child(label_text(steps::MCP_COMMAND, c.text, &t).font_family(t.fonts.mono))
        .child(
            Button::secondary("ob-copy", "Copy", t)
                .track_focus(&shell.focus.copy)
                .on_click(|_, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(steps::MCP_COMMAND.to_string()))
                }),
        );
    div()
        .flex()
        .flex_col()
        .gap(px(16.0))
        .child(label_text("Add Crastu to Claude Code", c.text, &t).font_weight(FontWeight::MEDIUM))
        .child(command)
        .child(caption("Defaults shown. Change them any time in Settings, Agents.", &t))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .p(px(16.0))
                .rounded(px(layout::RADIUS_CARD))
                .border_1()
                .border_color(c.border.rgb())
                .child(label_text("AI provider, optional", c.text, &t).font_weight(FontWeight::MEDIUM))
                .child(caption("Add your own key for smarter dictation cleanup and clip scoring. Crastu works without one.", &t))
                .child(caption("Adding a key comes with Settings > AI providers.", &t)),
        )
}

pub(super) fn welcome(t: Tokens) -> gpui::Div {
    let c = t.colors;
    let card = |name: &str, about: &str| {
        div()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .p(px(14.0))
            .rounded(px(layout::RADIUS_CARD))
            .border_1()
            .border_color(c.border.rgb())
            .child(label_text(name.to_string(), c.text, &t).font_weight(FontWeight::MEDIUM))
            .child(caption(about.to_string(), &t))
    };
    div()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .child(card(
            "Dictation",
            "Hold a key, talk, and clean text lands in any app.",
        ))
        .child(card(
            "Voice",
            "Turn a script into a voiceover with preset voices.",
        ))
        .child(card(
            "Auto-clipper",
            "Find the best moments in a long video and reframe them.",
        ))
        .child(card(
            "Editor",
            "A timeline for clips, voice, music and captions.",
        ))
}

pub(super) fn done(ob: &Onboarding, t: Tokens) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(label_text(steps::try_it(ob).0, t.colors.text_2, &t))
        .child(Kbd::new(steps::shortcut_label(&ob.shortcut), t))
        .child(label_text(steps::try_it(ob).1, t.colors.text_2, &t))
}
