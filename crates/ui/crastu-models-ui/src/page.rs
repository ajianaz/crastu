//! The Settings, Models and licences page: the Offline notice, the summary, one card per model
//! (licence badge, state, progress, the button that fits) and the Remove confirmation. It draws
//! `ModelsModel` and sends `Intent`s; every callback names what it means and the owner applies it
//! to the state as it is then.

use crastu_controls::DialogInput;
use crastu_models_model::{Intent, ModelRow, ModelState, ModelsModel};
use crastu_ui::tokens::{Tokens, layout};
use crastu_widgets::{BadgeView, Button, DialogView, label_text};
use gpui::{AnyElement, FontWeight, SharedString, div, prelude::*, px};

use crate::pane::{ModelsPane, ModelsSend};

/// The height of the progress bar's track.
const BAR_HEIGHT: f32 = 6.0;

fn caption(text: impl Into<SharedString>, t: &Tokens) -> gpui::Div {
    label_text(text, t.colors.text_2, t).text_size(px(12.0))
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

/// The content area of the page, drawn from the pane's state.
pub fn page(pane: &ModelsPane, t: Tokens, send: &ModelsSend) -> gpui::Stateful<gpui::Div> {
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
            label_text("Models and licences", t.colors.text, &t)
                .text_size(px(24.0))
                .font_weight(FontWeight::SEMIBOLD),
        )
        .child(caption(
            "Models run on this computer. Each one states its licence; every model here may be used commercially, some with extra terms.",
            &t,
        ))
        .child(caption(model.summary(), &t));
    if model.offline {
        page = page.child(offline_notice(t));
    }
    if let Some(why) = &model.load_error {
        page = page.child(error_banner(pane, why, t, send));
    }
    page.child(list(pane, t, send))
}

fn offline_notice(t: Tokens) -> gpui::Div {
    card(&t)
        .border_color(t.colors.warn.rgb())
        .child(
            label_text("Offline is on.", t.colors.warn, &t).font_weight(FontWeight::MEDIUM),
        )
        .child(caption(
            "Downloads are disabled. Models that are installed keep working. Turn Offline off in Settings, Privacy to download.",
            &t,
        ))
}

fn error_banner(pane: &ModelsPane, why: &str, t: Tokens, send: &ModelsSend) -> gpui::Div {
    let refresh = send.clone();
    card(&t)
        .border_color(t.colors.danger.rgb())
        .child(label_text(why.to_string(), t.colors.danger, &t))
        .child(
            div().child(
                Button::secondary("models-refresh", "Try again", t)
                    .track_focus(&pane.focus.refresh)
                    .on_click(move |_, w, a| refresh(Intent::Refresh, w, a)),
            ),
        )
}

fn list(pane: &ModelsPane, t: Tokens, send: &ModelsSend) -> gpui::Div {
    let model = &pane.model;
    if model.rows.is_empty() {
        return div().child(caption("The catalogue has no models.", &t));
    }
    let mut rows = div().flex().flex_col().w_full().gap(px(10.0));
    for (i, row) in model.rows.iter().enumerate() {
        rows = rows.child(model_card(pane, model, i, row, t, send));
    }
    rows
}

fn model_card(
    pane: &ModelsPane,
    model: &ModelsModel,
    i: usize,
    row: &ModelRow,
    t: Tokens,
    send: &ModelsSend,
) -> gpui::Div {
    let c = t.colors;
    let head = div()
        .flex()
        .items_center()
        .gap(px(10.0))
        .child(label_text(row.name.clone(), c.text, &t).font_weight(FontWeight::MEDIUM))
        .child(BadgeView::new(row.badge, t))
        .child(div().flex_1())
        .child(caption(format!("{} · {}", row.kind, row.size_text()), &t));
    let mut body = card(&t).child(head).child(caption(licence_line(row), &t));
    if let Some(note) = &row.licence_note {
        body = body.child(caption(note.clone(), &t));
    }
    let status_colour = match row.state {
        ModelState::Failed(_) => c.danger,
        ModelState::Installed => c.accent,
        _ => c.text_2,
    };
    body = body.child(label_text(row.status_text(), status_colour, &t).text_size(px(12.0)));
    if let Some(fraction) = row.fraction() {
        body = body.child(progress_bar(fraction, &t));
    }
    let mut actions = div().flex().items_center().gap(px(10.0));
    actions = actions.child(action_button(pane, model, i, row, t, send));
    if let Some(reason) = model.download_blocked(row) {
        actions = actions.child(caption(reason, &t));
    }
    body.child(actions)
}

fn licence_line(row: &ModelRow) -> String {
    match &row.attribution {
        Some(a) => format!("{}. {a}", row.licence),
        None => row.licence.clone(),
    }
}

/// A bar from the tokens: the track is the border colour, the fill the accent.
fn progress_bar(fraction: f32, t: &Tokens) -> gpui::Div {
    div()
        .w_full()
        .h(px(BAR_HEIGHT))
        .rounded_full()
        .bg(t.colors.border.rgb())
        .child(
            div()
                .h_full()
                .rounded_full()
                .bg(t.colors.accent.rgb())
                .w(gpui::relative(fraction.clamp(0.0, 1.0))),
        )
}

/// The one button of a row: Download, Cancel or Remove, with its disabled state.
fn action_button(
    pane: &ModelsPane,
    model: &ModelsModel,
    i: usize,
    row: &ModelRow,
    t: Tokens,
    send: &ModelsSend,
) -> gpui::Div {
    let id = SharedString::from(format!("model-action-{}", row.id));
    let model_id = row.id.clone();
    let send = send.clone();
    let focus = pane.focus.rows.get(i).map(|f| &f.action);
    let mut button = match &row.state {
        ModelState::NotInstalled | ModelState::Failed(_) => {
            let label = if matches!(row.state, ModelState::Failed(_)) {
                "Try again"
            } else {
                "Download"
            };
            Button::primary(id, label, t)
                .disabled(!model.can_download(row))
                .on_click(move |_, w, a| send(Intent::Download(model_id.clone()), w, a))
        }
        ModelState::Downloading { cancelling, .. } => Button::secondary(id, "Cancel", t)
            .disabled(*cancelling)
            .on_click(move |_, w, a| send(Intent::Cancel(model_id.clone()), w, a)),
        ModelState::Verifying => Button::secondary(id, "Cancel", t).disabled(true),
        ModelState::Installed => Button::danger(id, "Remove", t)
            .on_click(move |_, w, a| send(Intent::AskRemove(model_id.clone()), w, a)),
    };
    if let Some(focus) = focus {
        button = button.track_focus(focus);
    }
    div().child(button)
}

/// The Remove confirmation, drawn over the whole window.
pub fn dialog(pane: &ModelsPane, t: Tokens, send: &ModelsSend) -> Option<AnyElement> {
    let model = &pane.model;
    let (title, about) = model.confirm_text().filter(|_| model.dialog.open)?;
    let (input, cancel, confirm) = (send.clone(), send.clone(), send.clone());
    Some(
        DialogView::new(title, t)
            .subtitle(about)
            .track_focus(&pane.focus.dialog)
            .on_input(move |i: DialogInput, w, a| input(Intent::Dialog(i), w, a))
            .action(
                Button::secondary("models-confirm-cancel", "Cancel", t)
                    .on_click(move |_, w, a| cancel(Intent::Dialog(DialogInput::Close), w, a)),
            )
            .action(
                Button::danger("models-confirm-ok", "Remove", t)
                    .on_click(move |_, w, a| confirm(Intent::Confirm, w, a)),
            )
            .into_any_element(),
    )
}
