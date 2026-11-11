//! The Core components page: buttons, text fields, choices, sliders and tabs.

use crastu_controls::{FieldState, State, TextModel, Variant};
use crastu_ui::tokens::{Tokens, layout};
use crastu_widgets::*;
use gpui::{Context, SharedString, div, prelude::*, px};

use crate::app::{Gallery, caption, heading, hook, hook0};

pub fn core_page(g: &Gallery, t: Tokens, cx: &mut Context<Gallery>) -> gpui::Div {
    let c = t.colors;
    let this = g;
    let mut buttons = div().flex().flex_col().gap(px(10.0));
    let mut header = div().flex().gap(px(12.0)).child(div().w(px(80.0)));
    for state in State::ALL {
        header = header.child(div().w(px(116.0)).child(caption(&t, &format!("{state:?}"))));
    }
    buttons = buttons.child(header);
    for (v, name, label) in [
        (Variant::Primary, "Primary", "Find clips"),
        (Variant::Secondary, "Secondary", "Replace"),
        (Variant::Ghost, "Ghost", "Cancel"),
        (Variant::Danger, "Danger", "Delete"),
    ] {
        let mut row = div()
            .flex()
            .items_center()
            .gap(px(12.0))
            .child(div().w(px(80.0)).child(caption(&t, name)));
        for state in State::ALL {
            let text = if state == State::Loading {
                "Working"
            } else {
                label
            };
            row = row.child(
                div().w(px(116.0)).child(
                    Button::new(SharedString::from(format!("{name}{state:?}")), text, v, t)
                        .state(state),
                ),
            );
        }
        buttons = buttons.child(row);
    }

    let field = |id: &'static str, state: FieldState, text: &str, help: &str| {
        div().w(px(190.0)).child(
            TextField::new(id, "Project name", &TextModel::new(text), t)
                .state(state)
                .help(help.to_string()),
        )
    };
    let fields = div()
        .flex()
        .gap(px(16.0))
        .child(field(
            "f1",
            FieldState::Default,
            "Podcast #12",
            "Shown in the project list",
        ))
        .child(field(
            "f2",
            FieldState::Focus,
            "Podcast #12",
            "Shown in the project list",
        ))
        .child(field(
            "f3",
            FieldState::Error,
            "",
            "A project name is required",
        ))
        .child(field(
            "f4",
            FieldState::Disabled,
            "Podcast #12",
            "Locked while exporting",
        ));

    let live_name = TextField::new("name", "Project name (type here)", &this.name, t)
        .placeholder("Name the project")
        .help("Shown in the project list")
        .track_focus(&this.name_focus)
        .on_edit(hook(cx, |g, e, cx| {
            g.name.apply(&e);
            cx.notify();
        }));

    div()
        .flex()
        .flex_col()
        .gap(px(24.0))
        .child(heading(&t, "Buttons"))
        .child(buttons)
        .child(heading(&t, "Text fields"))
        .child(fields)
        .child(div().w(px(300.0)).child(live_name))
        .child(heading(&t, "Choices"))
        .child(
            div()
                .flex()
                .items_start()
                .gap(px(40.0))
                .child(
                    Choices::new(
                        "ratio",
                        ChoiceKind::Segmented,
                        &["9:16", "1:1", "16:9"],
                        &this.ratio,
                        t,
                    )
                    .track_focus(&this.ratio_focus)
                    .on_select(hook(cx, |g, i, cx| {
                        g.ratio.apply(&i);
                        cx.notify();
                    })),
                )
                .child(
                    div().w(px(220.0)).h(px(150.0)).child(
                        SelectView::new(
                            "lang",
                            "Language",
                            &["Indonesian", "English", "Detect automatically"],
                            &this.language,
                            t,
                        )
                        .track_focus(&this.language_focus)
                        .on_change(hook(cx, |g, m, cx| {
                            g.language.apply(&m);
                            cx.notify();
                        })),
                    ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(10.0))
                        .child(
                            Checkbox::new("follow", "Follow the active speaker", this.follow, t)
                                .track_focus(&this.follow_focus)
                                .on_toggle(hook0(cx, |g, cx| {
                                    g.follow = !g.follow;
                                    cx.notify();
                                })),
                        )
                        .child(
                            Checkbox::new("filler", "Cut filler words", this.filler, t)
                                .track_focus(&this.filler_focus)
                                .on_toggle(hook0(cx, |g, cx| {
                                    g.filler = !g.filler;
                                    cx.notify();
                                })),
                        )
                        .child(
                            Checkbox::new("dia", "Diarisation (needs a model)", false, t)
                                .disabled(true),
                        )
                        .child(
                            Switch::new("keep", "Keep model loaded", this.keep_loaded, t)
                                .track_focus(&this.keep_focus)
                                .on_toggle(hook0(cx, |g, cx| {
                                    g.keep_loaded = !g.keep_loaded;
                                    cx.notify();
                                })),
                        )
                        .child(
                            Checkbox::new("focus", "Focus ring (forced)", true, t).show_focus(true),
                        ),
                ),
        )
        .child(heading(&t, "Sliders and tabs"))
        .child(
            div()
                .flex()
                .items_start()
                .gap(px(40.0))
                .child(
                    div()
                        .w(px(420.0))
                        .flex()
                        .flex_col()
                        .gap(px(16.0))
                        .child(
                            SliderView::new(
                                "vol",
                                "Voiceover volume",
                                format!("{:.1} dB", this.volume.value),
                                this.volume,
                                t,
                            )
                            .track_focus(&this.volume_focus)
                            .on_change(hook(cx, |g, s, cx| {
                                g.volume.apply(s);
                                cx.notify();
                            })),
                        )
                        .child(
                            RangeSliderView::new(
                                "clip",
                                "Clip length",
                                format!("{:.0} - {:.0} s", this.clip.low, this.clip.high),
                                this.clip,
                                t,
                            )
                            .track_focus(&this.clip_focus.0, &this.clip_focus.1)
                            .on_change(hook(cx, |g, r, cx| {
                                g.clip.apply(r);
                                cx.notify();
                            })),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .child(
                            Choices::new(
                                "tabs",
                                ChoiceKind::Tabs,
                                &["Transcript", "Captions", "Framing"],
                                &this.tab,
                                t,
                            )
                            .track_focus(&this.tab_focus)
                            .on_select(hook(cx, |g, i, cx| {
                                g.tab.apply(&i);
                                cx.notify();
                            })),
                        )
                        .child(label_text(
                            [
                                "Word-level text that follows the playhead.",
                                "Caption styles and timing.",
                                "Crop and safe area.",
                            ][this.tab.selected],
                            c.text_2,
                            &t,
                        )),
                ),
        )
        .child(caption(
            &t,
            &format!(
                "Hit target {} px",
                layout::hit_target(this.appearance.density)
            ),
        ))
}
