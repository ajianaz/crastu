//! The command palette overlay (Cmd+K): a scrim, an input line and the matching commands.
//! It draws the `Palette` state and sends `PaletteInput`s; running a command is the shell's job.

use crastu_controls::Edit;
use crastu_ui::tokens::{Tokens, layout};
use crastu_widgets::{Kbd, label_text};
use gpui::{Context, ElementId, FontWeight, Hsla, MouseButton, div, prelude::*, px};

use super::Shell;
use crate::palette::{PaletteInput, entries_for};

pub fn overlay(
    shell: &Shell,
    t: Tokens,
    cx: &mut Context<Shell>,
) -> Option<gpui::Stateful<gpui::Div>> {
    if !shell.palette.open {
        return None;
    }
    let c = t.colors;
    let all = entries_for(shell.model.state().network.locked_by_env);
    let rows = shell.palette.rows(&all);
    let send = Shell::palette_sender(cx);

    let query = shell.palette.query.text();
    // The text on each side of the cursor, so the caret is drawn where Left and Right put it.
    let (before, after): (String, String) = {
        let chars: Vec<char> = query.chars().collect();
        let at = shell.palette.query.cursor().min(chars.len());
        (chars[..at].iter().collect(), chars[at..].iter().collect())
    };
    let input = div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .h(px(44.0))
        .px(px(16.0))
        .border_b_1()
        .border_color(c.border.rgb())
        .child(if query.is_empty() {
            label_text("Type a command or a place to go", c.text_3, &t)
        } else {
            label_text(before, c.text, &t)
        })
        .child(div().w(px(1.0)).h(px(16.0)).bg(c.accent.rgb()))
        .child(label_text(after, c.text, &t))
        .child(div().flex_1())
        .child(Kbd::new("esc", t));

    let mut list = div().flex().flex_col().p(px(6.0)).gap(px(2.0));
    if rows.is_empty() {
        list = list.child(div().px(px(10.0)).py(px(8.0)).child(label_text(
            "No command matches",
            c.text_3,
            &t,
        )));
    }
    for (n, i) in rows.iter().enumerate() {
        let e = &all[*i];
        let selected = n == shell.palette.selected;
        let action = e.action.clone();
        let (send_row, hover_bg) = (send.clone(), c.raised);
        let mut row = div()
            .id(ElementId::NamedInteger("palette-row".into(), n as u64))
            .flex()
            .items_center()
            .gap(px(10.0))
            .h(px(32.0))
            .px(px(10.0))
            .rounded(px(layout::RADIUS_CONTROL))
            .cursor_pointer()
            .hover(move |s| s.bg(hover_bg.rgb()))
            .child(
                label_text(e.group, c.text_3, &t)
                    .text_size(px(11.0))
                    .w(px(78.0)),
            )
            .child(
                label_text(e.label.clone(), c.text, &t).font_weight(if selected {
                    FontWeight::MEDIUM
                } else {
                    FontWeight::NORMAL
                }),
            )
            .on_click(move |_, window, app| {
                // A click chooses this row: select it, then press Enter for it.
                send_row(PaletteInput::Choose(action.clone()), window, app);
            });
        if selected {
            row = row.bg(c.raised.rgb());
        }
        list = list.child(row);
    }

    let key_send = send.clone();
    let panel = div()
        .id("palette")
        .track_focus(&shell.focus.palette)
        .occlude()
        .w(px(560.0))
        .mt(px(96.0))
        .rounded(px(layout::RADIUS_CARD))
        .border_1()
        .border_color(c.border.rgb())
        .bg(c.surface.rgb())
        .shadow_lg()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_key_down(move |ev, window, app| {
            let key = ev.keystroke.key.as_str();
            let m = &ev.keystroke.modifiers;
            // Tab would move the focus out of the open palette: it stays inside.
            if key == "tab" {
                app.stop_propagation();
                return;
            }
            let input = if matches!(key, "up" | "down" | "enter" | "escape") {
                Some(PaletteInput::Key(key.to_string()))
            } else if m.platform || m.control {
                None
            } else {
                Edit::from_key(key, ev.keystroke.key_char.as_deref()).map(PaletteInput::Edit)
            };
            if let Some(input) = input {
                key_send(input, window, app);
                app.stop_propagation();
            }
        })
        .child(input)
        .child(list);

    let scrim: Hsla = c.bg.rgb().into();
    Some(
        div()
            .id("palette-scrim")
            .occlude()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .items_start()
            .justify_center()
            .bg(scrim.opacity(0.6))
            .on_mouse_down(MouseButton::Left, move |_, window, app| {
                send(PaletteInput::Close, window, app)
            })
            .child(panel),
    )
}
