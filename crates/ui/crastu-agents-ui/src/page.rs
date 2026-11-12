//! The Settings, Agents page: pairing, the one-time token box, the agents with their overrides,
//! the default policy, the recent calls and the confirmation dialog. It draws `AgentsModel` and sends
//! `Intent`s; every callback names what it means and the owner applies it to the state as it is
//! then.

use crastu_agents_model::{
    AgentRow, AgentsModel, CallRow, Class, Intent, Reveal, TOKEN_ENV, default_labels,
    default_selected, override_labels, override_selected, short_time,
};
use crastu_controls::{Choice, DialogInput, Edit, FieldState};
use crastu_ui::tokens::{Color, Tokens, layout};
use crastu_widgets::{Button, ChoiceKind, Choices, DialogView, TextField, label_text};
use gpui::{AnyElement, FocusHandle, FontWeight, SharedString, div, prelude::*, px};

use crate::pane::{AgentSend, AgentsPane};

/// A caption line in the secondary text colour.
fn caption(text: impl Into<SharedString>, t: &Tokens) -> gpui::Div {
    label_text(text, t.colors.text_2, t).text_size(px(12.0))
}

/// The content area of the page, drawn from the pane's state.
pub fn page(pane: &AgentsPane, t: Tokens, send: &AgentSend) -> gpui::Stateful<gpui::Div> {
    let model = &pane.model;
    let mut page = div()
        .id("content")
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .overflow_y_scroll()
        .p(px(24.0))
        .gap(px(20.0))
        .bg(t.colors.bg.rgb())
        .child(
            label_text("Agents (MCP)", t.colors.text, &t)
                .text_size(px(24.0))
                .font_weight(FontWeight::SEMIBOLD),
        )
        .child(caption(
            "AI agents reach Crastu through MCP. Each paired agent has its own token; a caller without one only ever gets the default policy.",
            &t,
        ))
        .child(pair_form(pane, model, t, send));
    if model.snapshot.policy_damaged {
        page = page.child(damaged_banner(pane, t, send));
    }
    if let Some(reveal) = &model.reveal {
        page = page.child(token_box(pane, reveal, t, send));
    }
    page.child(agent_list(pane, model, t, send))
        .child(default_policy(pane, model, t, send))
        .child(recent_calls(pane, model, t, send))
}

fn section(name: &str, t: &Tokens) -> gpui::Div {
    div().flex().flex_col().items_start().gap(px(10.0)).child(
        label_text(name.to_string(), t.colors.text, t)
            .text_size(px(16.0))
            .font_weight(FontWeight::SEMIBOLD),
    )
}

fn card(t: &Tokens) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .p(px(16.0))
        .rounded(px(layout::RADIUS_CARD))
        .border_1()
        .border_color(t.colors.border.rgb())
        .bg(t.colors.surface.rgb())
}

fn pill(text: &str, fg: Color, fill: Color, t: &Tokens) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .h(px(22.0))
        .px(px(10.0))
        .rounded_full()
        .border_1()
        .border_color(fg.rgb())
        .bg(fill.rgb())
        .child(label_text(text.to_string(), fg, t).text_size(px(12.0)))
}

fn damaged_banner(pane: &AgentsPane, t: Tokens, send: &AgentSend) -> gpui::Div {
    let reset = send.clone();
    card(&t)
        .border_color(t.colors.danger.rgb())
        .child(
            label_text(
                "The approval policy is damaged: the host allows reads only.",
                t.colors.danger,
                &t,
            )
            .font_weight(FontWeight::MEDIUM),
        )
        .child(caption(
            "Changes and deletions are refused until the policy is reset. The settings below are not what the host applies.",
            &t,
        ))
        .child(
            div().child(
                Button::danger("policy-reset", "Reset policy to defaults", t)
                    .track_focus(&pane.focus.reset)
                    .on_click(move |_, w, a| reset(Intent::AskResetPolicy, w, a)),
            ),
        )
}

fn pair_form(pane: &AgentsPane, model: &AgentsModel, t: Tokens, send: &AgentSend) -> gpui::Div {
    let (edit, pair) = (send.clone(), send.clone());
    let mut field = TextField::new("agent-name", "Agent name", &model.name, t)
        .placeholder("claude-code")
        .track_focus(&pane.focus.name)
        .on_edit(move |e: Edit, w, a| edit(Intent::Name(e), w, a));
    field = match model.name_error() {
        Some(problem) => field.state(FieldState::Error).help(problem),
        None => field.help("Letters a-z, digits, '-', '_' and '.'; up to 64 characters."),
    };
    section("Pair an agent", &t)
        .child(div().w(px(360.0)).child(field))
        .child(
            Button::primary("agent-pair", "Pair", t)
                .track_focus(&pane.focus.pair)
                .disabled(!model.can_pair())
                .on_click(move |_, w, a| pair(Intent::Pair, w, a)),
        )
}

fn token_box(pane: &AgentsPane, reveal: &Reveal, t: Tokens, send: &AgentSend) -> gpui::Div {
    let c = t.colors;
    let (copy, done) = (send.clone(), send.clone());
    let heading = if reveal.rotated {
        format!("New token for {}", reveal.agent)
    } else {
        format!("Token for {}", reveal.agent)
    };
    let mut notes = vec![
        "Shown once. Crastu keeps only a hash of it, so it cannot show it again; rotate the token if it is lost.".to_string(),
        format!(
            "Put it in the \"env\" block of the client's own MCP configuration as {TOKEN_ENV}. Do not put it on the command line: the shell history and the process list can expose it."
        ),
    ];
    if reveal.rotated {
        notes.insert(0, "The old token no longer works.".to_string());
    }
    let mut boxed = card(&t)
        .border_color(c.accent.rgb())
        .child(label_text(heading, c.text, &t).font_weight(FontWeight::MEDIUM))
        .child(
            div()
                .px(px(12.0))
                .py(px(10.0))
                .rounded(px(layout::RADIUS_CONTROL))
                .border_1()
                .border_color(c.border.rgb())
                .bg(c.bg.rgb())
                .child(
                    label_text(reveal.token.expose().to_string(), c.text, &t)
                        .font_family(t.fonts.mono),
                ),
        );
    for n in notes {
        boxed = boxed.child(caption(n, &t));
    }
    boxed.child(
        div()
            .flex()
            .gap(px(8.0))
            .child(
                Button::secondary("agent-copy", "Copy", t)
                    .track_focus(&pane.focus.copy)
                    .on_click(move |_, w, a| copy(Intent::CopyToken, w, a)),
            )
            .child(
                Button::primary("agent-token-done", "Done", t)
                    .track_focus(&pane.focus.done)
                    .on_click(move |_, w, a| done(Intent::DismissToken, w, a)),
            ),
    )
}

fn agent_list(pane: &AgentsPane, model: &AgentsModel, t: Tokens, send: &AgentSend) -> gpui::Div {
    let mut list = section("Paired agents", &t);
    if model.snapshot.agents.is_empty() {
        return list.child(caption(
            "No agents are paired yet. Pair one above, then add its token to the client's configuration.",
            &t,
        ));
    }
    let mut rows = div().flex().flex_col().w_full().gap(px(10.0));
    for (i, agent) in model.snapshot.agents.iter().enumerate() {
        rows = rows.child(agent_row(pane, i, agent, t, send));
    }
    list = list.w_full().child(rows);
    list
}

fn agent_row(pane: &AgentsPane, i: usize, a: &AgentRow, t: Tokens, send: &AgentSend) -> gpui::Div {
    let c = t.colors;
    let state = if a.revoked {
        pill("Revoked", c.danger, c.danger_soft, &t)
    } else {
        pill("Paired", c.accent, c.accent_soft, &t)
    };
    let last = match &a.last_call {
        Some(call) => format!("Last call {}: {}", short_time(&call.at), call.tool),
        None => "No calls yet".to_string(),
    };
    let head = div()
        .flex()
        .items_center()
        .gap(px(10.0))
        .child(
            label_text(a.name.clone(), c.text, &t)
                .font_weight(FontWeight::MEDIUM)
                .font_family(t.fonts.mono),
        )
        .child(state)
        .child(div().flex_1())
        .child(caption(format!("Paired {}", short_time(&a.created_at)), &t));
    let mut body = card(&t).child(
        div()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(head)
            .child(caption(last, &t)),
    );
    if a.revoked {
        return body.child(caption(
            "Its token no longer works. Pair a new agent under another name.",
            &t,
        ));
    }
    let Some(focus) = pane.focus.rows.get(i) else {
        return body;
    };
    let (rotate, revoke) = (send.clone(), send.clone());
    let rotate_ok = pane.model.can_rotate();
    let (name_a, name_b) = (a.name.clone(), a.name.clone());
    body = body
        .child(override_row(a, Class::Change, &focus.change, i, t, send))
        .child(override_row(a, Class::Delete, &focus.delete, i, t, send))
        .child(
            div()
                .flex()
                .gap(px(8.0))
                .child(
                    Button::secondary(SharedString::from(format!("agent-rotate-{i}")), "Rotate", t)
                        .disabled(!rotate_ok)
                        .track_focus(&focus.rotate)
                        .on_click(move |_, w, x| rotate(Intent::Rotate(name_a.clone()), w, x)),
                )
                .child(
                    Button::danger(SharedString::from(format!("agent-revoke-{i}")), "Revoke", t)
                        .track_focus(&focus.revoke)
                        .on_click(move |_, w, x| revoke(Intent::AskRevoke(name_b.clone()), w, x)),
                ),
        );
    body
}

fn override_row(
    a: &AgentRow,
    class: Class,
    focus: &FocusHandle,
    i: usize,
    t: Tokens,
    send: &AgentSend,
) -> gpui::Div {
    let labels = override_labels(class);
    let choice = Choice::new(labels.len(), override_selected(a, class));
    let (send, name) = (send.clone(), a.name.clone());
    let id = SharedString::from(format!("agent-{}-{i}", class.label()));
    div()
        .flex()
        .items_center()
        .gap(px(12.0))
        .child(div().w(px(96.0)).child(caption(class.label(), &t)))
        .child(
            Choices::new(id, ChoiceKind::Segmented, &labels, &choice, t)
                .track_focus(focus)
                .on_select(move |input, w, a| {
                    send(Intent::Override(name.clone(), class, input), w, a)
                }),
        )
}

fn default_policy(
    pane: &AgentsPane,
    model: &AgentsModel,
    t: Tokens,
    send: &AgentSend,
) -> gpui::Div {
    let mut sec = section("Default policy", &t).child(caption(
        "Applies to every agent without an override of its own, and to callers without a token. Reads always run automatically.",
        &t,
    ));
    for class in Class::ALL {
        let labels = default_labels(class);
        let choice = Choice::new(
            labels.len(),
            default_selected(&model.snapshot.defaults, class),
        );
        let focus = match class {
            Class::Change => &pane.focus.default_change,
            Class::Delete => &pane.focus.default_delete,
        };
        let send = send.clone();
        sec = sec.child(
            div()
                .flex()
                .flex_col()
                .items_start()
                .gap(px(6.0))
                .child(label_text(class.label(), t.colors.text, &t).font_weight(FontWeight::MEDIUM))
                .child(caption(class.about(), &t))
                .child(
                    Choices::new(
                        SharedString::from(format!("default-{}", class.label())),
                        ChoiceKind::Segmented,
                        &labels,
                        &choice,
                        t,
                    )
                    .track_focus(focus)
                    .on_select(move |input, w, a| send(Intent::Default(class, input), w, a)),
                ),
        );
    }
    sec
}

fn call_line(call: &CallRow, t: &Tokens) -> gpui::Div {
    let c = t.colors;
    let who = if call.paired {
        call.agent.clone()
    } else {
        format!("{} (unpaired)", call.agent)
    };
    div()
        .flex()
        .items_baseline()
        .gap(px(12.0))
        .child(
            div().w(px(120.0)).child(
                label_text(short_time(&call.at), c.text_3, t)
                    .font_family(t.fonts.mono)
                    .text_size(px(12.0)),
            ),
        )
        .child(div().w(px(180.0)).child(label_text(who, c.text_2, t)))
        .child(
            div()
                .flex_1()
                .child(label_text(call.tool.clone(), c.text, t).font_family(t.fonts.mono)),
        )
        .child(label_text(
            format!("{}, {}", call.decision, call.outcome),
            c.text_2,
            t,
        ))
}

fn recent_calls(pane: &AgentsPane, model: &AgentsModel, t: Tokens, send: &AgentSend) -> gpui::Div {
    let refresh = send.clone();
    let mut sec = section("Recent calls", &t);
    if model.snapshot.recent.is_empty() {
        sec = sec.child(caption("No agent has called Crastu yet.", &t));
    } else {
        let mut lines = div().flex().flex_col().gap(px(6.0));
        for call in &model.snapshot.recent {
            lines = lines.child(call_line(call, &t));
        }
        sec = sec.child(lines);
    }
    sec.child(
        Button::ghost("agents-refresh", "Refresh", t)
            .track_focus(&pane.focus.refresh)
            .on_click(move |_, w, a| refresh(Intent::Refresh, w, a)),
    )
}

/// The confirmation (revoke, reset the policy, Auto), drawn over the whole window.
pub fn dialog(pane: &AgentsPane, t: Tokens, send: &AgentSend) -> Option<AnyElement> {
    let model = &pane.model;
    let what = model.confirm.as_ref().filter(|_| model.dialog.open)?;
    let (input, cancel, confirm) = (send.clone(), send.clone(), send.clone());
    Some(
        DialogView::new(what.title(), t)
            .subtitle(what.subtitle())
            .track_focus(&pane.focus.dialog)
            .on_input(move |i: DialogInput, w, a| input(Intent::Dialog(i), w, a))
            .action(
                Button::secondary("confirm-cancel", "Cancel", t)
                    .on_click(move |_, w, a| cancel(Intent::Dialog(DialogInput::Close), w, a)),
            )
            .action(
                Button::danger("confirm-ok", what.action(), t)
                    .on_click(move |_, w, a| confirm(Intent::Confirm, w, a)),
            )
            .into_any_element(),
    )
}
