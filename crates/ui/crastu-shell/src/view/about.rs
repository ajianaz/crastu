//! Settings > About: version, operating system, the log folder, and the two buttons. A click only
//! sends an [`AboutIntent`]; the shell applies it to the platform as it is then (no state is
//! captured by the callbacks) and shows the toast and writes the clipboard.

use std::rc::Rc;

use crastu_controls::ToastKind;
use crastu_ui::tokens::{Tokens, layout};
use crastu_widgets::{Button, label_text};
use gpui::{App, ClipboardItem, Context, FontWeight, Window, div, prelude::*, px};

use super::pages::{page, title};
use super::{Shell, caption};
use crate::about::{AboutEffect, AboutInfo, AboutIntent, apply};

type AboutSend = Rc<dyn Fn(AboutIntent, &mut Window, &mut App) + 'static>;

impl Shell {
    fn about_sender(cx: &Context<Self>) -> AboutSend {
        let owner = cx.entity().downgrade();
        Rc::new(move |intent, _, app| {
            let _ = owner.update(app, |shell, cx| shell.about_intent(intent, cx));
        })
    }

    fn about_intent(&mut self, intent: AboutIntent, cx: &mut Context<Self>) {
        for effect in apply(intent, &*self.platform) {
            match effect {
                AboutEffect::Clipboard(text) => {
                    cx.write_to_clipboard(ClipboardItem::new_string(text))
                }
                AboutEffect::Toast(ok, message) => {
                    let kind = if ok {
                        ToastKind::Success
                    } else {
                        ToastKind::Error
                    };
                    self.toasts.push(kind, &message, None);
                }
            }
        }
        cx.notify();
    }

    pub(super) fn about_page(&self, t: Tokens, cx: &Context<Self>) -> gpui::Stateful<gpui::Div> {
        let info = self.platform.about();
        let send = Self::about_sender(cx);
        let (open, copy) = (send.clone(), send);
        page(t)
            .child(title("About", &t))
            .child(facts(&info, t))
            .child(action(
                Button::secondary("about-open-logs", "Open logs", t)
                    .track_focus(&self.focus.open_logs)
                    .on_click(move |_, w, a| open(AboutIntent::OpenLogs, w, a)),
                "Shows the folder with the logs of every run in the file manager.",
                t,
            ))
            .child(action(
                Button::secondary("about-copy-diagnostics", "Copy diagnostics", t)
                    .track_focus(&self.focus.copy_diagnostics)
                    .on_click(move |_, w, a| copy(AboutIntent::CopyDiagnostics, w, a)),
                "Copies the version, the system, the last events and how the previous run ended. Keys are removed. Nothing is sent; you decide where to paste it.",
                t,
            ))
    }
}

/// A button with a line that says what it does, for the person who cannot see the layout.
fn action(button: Button, what: &str, t: Tokens) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .items_start()
        .gap(px(6.0))
        .child(button)
        .child(caption(what.to_string(), &t))
}

fn facts(info: &AboutInfo, t: Tokens) -> gpui::Div {
    let c = t.colors;
    let log_dir = info
        .log_dir
        .clone()
        .unwrap_or_else(|| "No log folder for this run".to_string());
    let row = |name: &str, value: String, mono: bool| {
        let mut v = label_text(value, c.text, &t);
        if mono {
            v = v.font_family(t.fonts.mono);
        }
        div()
            .flex()
            .gap(px(16.0))
            .child(
                div()
                    .w(px(140.0))
                    .child(label_text(name.to_string(), c.text_2, &t)),
            )
            .child(div().flex_1().min_w_0().child(v))
    };
    div()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .p(px(16.0))
        .rounded(px(layout::RADIUS_CARD))
        .border_1()
        .border_color(c.border.rgb())
        .bg(c.surface.rgb())
        .child(
            label_text("Crastu", c.text, &t)
                .text_size(px(16.0))
                .font_weight(FontWeight::SEMIBOLD),
        )
        .child(row("Version", info.version.clone(), false))
        .child(row("System", info.os.clone(), false))
        .child(row("Log folder", log_dir, true))
}
