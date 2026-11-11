//! Every base control in every documented state, in the chosen theme. The Theme row switches
//! the theme live. Run with `cargo run -p crastu-widgets --example gallery` (add `--smoke` to
//! quit after a moment). The full gallery with the media and feedback components comes later.

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod imp {
    use std::time::Duration;

    use crastu_app::{Appearance, Theme};
    use crastu_controls::{
        Choice, FieldState, RangeSlider, SelectModel, Slider, State, TextModel, Variant,
    };
    use crastu_ui::tokens::{Tokens, layout};
    use crastu_widgets::*;
    use gpui::{
        App, Bounds, Context, FocusHandle, FontWeight, IntoElement, Render, SharedString, Window,
        WindowBounds, WindowOptions, div, prelude::*, px, size,
    };
    use gpui_platform::application;

    struct Gallery {
        appearance: Appearance,
        installed_fonts: Vec<String>,
        theme_choice: Choice,
        name: TextModel,
        name_focus: FocusHandle,
        language: SelectModel,
        language_focus: FocusHandle,
        ratio: Choice,
        ratio_focus: FocusHandle,
        tab: Choice,
        tab_focus: FocusHandle,
        theme_focus: FocusHandle,
        follow_focus: FocusHandle,
        filler_focus: FocusHandle,
        keep_focus: FocusHandle,
        follow: bool,
        filler: bool,
        keep_loaded: bool,
        volume: Slider,
        volume_focus: FocusHandle,
        clip: RangeSlider,
        clip_focus: (FocusHandle, FocusHandle),
    }

    impl Gallery {
        fn new(cx: &mut Context<Self>) -> Gallery {
            Gallery {
                appearance: Appearance::default(),
                installed_fonts: cx.text_system().all_font_names(),
                theme_choice: Choice::new(3, 0),
                name: TextModel::new("Podcast #12").with_max_chars(40),
                name_focus: cx.focus_handle(),
                language: SelectModel::new(3, 0),
                language_focus: cx.focus_handle(),
                ratio: Choice::new(3, 0),
                ratio_focus: cx.focus_handle(),
                tab: Choice::new(3, 0),
                tab_focus: cx.focus_handle(),
                theme_focus: cx.focus_handle(),
                follow_focus: cx.focus_handle(),
                filler_focus: cx.focus_handle(),
                keep_focus: cx.focus_handle(),
                follow: true,
                filler: false,
                keep_loaded: true,
                volume: Slider::new(-60.0, 6.0, 0.5, -6.0),
                volume_focus: cx.focus_handle(),
                clip: RangeSlider::new(5.0, 90.0, 1.0, 5.0, 30.0, 60.0),
                clip_focus: (cx.focus_handle(), cx.focus_handle()),
            }
        }
    }

    /// A change callback for a control: runs `f` on the gallery with the new value.
    fn hook<V: 'static>(
        cx: &Context<Gallery>,
        f: impl Fn(&mut Gallery, V, &mut Context<Gallery>) + 'static,
    ) -> impl Fn(V, &mut Window, &mut App) + 'static {
        let this = cx.entity().downgrade();
        move |v, _, app| {
            this.update(app, |g, cx| f(g, v, cx)).ok();
        }
    }

    fn heading(t: &Tokens, text: &str) -> gpui::Div {
        label_text(text.to_string(), t.colors.text, t)
            .text_size(px(16.0))
            .font_weight(FontWeight::SEMIBOLD)
    }

    fn caption(t: &Tokens, text: &str) -> gpui::Div {
        label_text(text.to_string(), t.colors.text_2, t).text_size(px(12.0))
    }

    impl Render for Gallery {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let t =
                Tokens::new(&self.appearance, false).with_installed_fonts(&self.installed_fonts);
            let c = t.colors;

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

            let live_name = TextField::new("name", "Project name (type here)", &self.name, t)
                .placeholder("Name the project")
                .help("Shown in the project list")
                .track_focus(&self.name_focus)
                .on_edit(hook(cx, |g, e, cx| {
                    g.name.apply(&e);
                    cx.notify();
                }));

            let theme_row = Choices::new(
                "theme",
                ChoiceKind::Segmented,
                &["Graphite", "Paper", "Signal"],
                &self.theme_choice,
                t,
            )
            .track_focus(&self.theme_focus)
            .on_select(hook(cx, |g, i, cx| {
                g.theme_choice.apply(&i);
                g.appearance.theme = Theme::ALL[g.theme_choice.selected];
                cx.notify();
            }));

            div()
                .id("page")
                .size_full()
                .overflow_y_scroll()
                .bg(c.bg.rgb())
                .p(px(24.0))
                .flex()
                .flex_col()
                .gap(px(24.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(16.0))
                        .child(heading(&t, "Core components"))
                        .child(theme_row),
                )
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
                                &self.ratio,
                                t,
                            )
                            .track_focus(&self.ratio_focus)
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
                                    &self.language,
                                    t,
                                )
                                .track_focus(&self.language_focus)
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
                                    Checkbox::new(
                                        "follow",
                                        "Follow the active speaker",
                                        self.follow,
                                        t,
                                    )
                                    .track_focus(&self.follow_focus)
                                    .on_toggle(hook(
                                        cx,
                                        |g, v, cx| {
                                            g.follow = v;
                                            cx.notify();
                                        },
                                    )),
                                )
                                .child(
                                    Checkbox::new("filler", "Cut filler words", self.filler, t)
                                        .track_focus(&self.filler_focus)
                                        .on_toggle(hook(cx, |g, v, cx| {
                                            g.filler = v;
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    Checkbox::new("dia", "Diarisation (needs a model)", false, t)
                                        .disabled(true),
                                )
                                .child(
                                    Switch::new("keep", "Keep model loaded", self.keep_loaded, t)
                                        .track_focus(&self.keep_focus)
                                        .on_toggle(hook(cx, |g, v, cx| {
                                            g.keep_loaded = v;
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    Checkbox::new("focus", "Focus ring (forced)", true, t)
                                        .show_focus(true),
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
                                        format!("{:.1} dB", self.volume.value),
                                        self.volume,
                                        t,
                                    )
                                    .track_focus(&self.volume_focus)
                                    .on_change(hook(
                                        cx,
                                        |g, s, cx| {
                                            g.volume.apply(s);
                                            cx.notify();
                                        },
                                    )),
                                )
                                .child(
                                    RangeSliderView::new(
                                        "clip",
                                        "Clip length",
                                        format!("{:.0} - {:.0} s", self.clip.low, self.clip.high),
                                        self.clip,
                                        t,
                                    )
                                    .track_focus(&self.clip_focus.0, &self.clip_focus.1)
                                    .on_change(hook(
                                        cx,
                                        |g, r, cx| {
                                            g.clip.apply(r);
                                            cx.notify();
                                        },
                                    )),
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
                                        &self.tab,
                                        t,
                                    )
                                    .track_focus(&self.tab_focus)
                                    .on_select(hook(
                                        cx,
                                        |g, i, cx| {
                                            g.tab.apply(&i);
                                            cx.notify();
                                        },
                                    )),
                                )
                                .child(label_text(
                                    [
                                        "Word-level text that follows the playhead.",
                                        "Caption styles and timing.",
                                        "Crop and safe area.",
                                    ][self.tab.selected],
                                    c.text_2,
                                    &t,
                                )),
                        ),
                )
                .child(caption(
                    &t,
                    &format!(
                        "Hit target {} px",
                        layout::hit_target(self.appearance.density)
                    ),
                ))
        }
    }

    pub fn main() {
        let smoke = std::env::args().any(|a| a == "--smoke");
        application().run(move |cx: &mut App| {
            crastu_ui::init(cx);
            let bounds = Bounds::centered(None, size(px(1100.0), px(900.0)), cx);
            let opened = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |_, cx| cx.new(Gallery::new),
            );
            if opened.is_err() {
                eprintln!("gallery: cannot open the window");
                std::process::exit(1);
            }
            cx.activate(true);
            if smoke {
                cx.spawn(async move |cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(1500))
                        .await;
                    cx.update(|cx| cx.quit());
                })
                .detach();
            }
        });
    }
}

fn main() {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    imp::main();
}
