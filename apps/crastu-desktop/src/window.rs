//! The placeholder window.

use std::time::Duration;

use gpui::{
    App, Bounds, Context, IntoElement, Render, TitlebarOptions, Window, WindowBounds,
    WindowOptions, div, prelude::*, px, rgb, size,
};
use gpui_platform::application;

/// The Graphite background from `docs/design/tokens.json`. Replaced by the token module of
/// `crastu-ui` (issue "design tokens"); the lint against literals starts there.
const BACKGROUND: u32 = 0x111214;

struct Blank;

impl Render for Blank {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().bg(rgb(BACKGROUND))
    }
}

/// Opens the window and runs until it is closed, or until `quit_after` has passed.
pub fn open(quit_after: Option<Duration>) {
    application().run(move |cx: &mut App| {
        crastu_ui::init(cx);
        let bounds = Bounds::centered(None, size(px(1280.0), px(800.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("Crastu".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_, cx| cx.new(|_| Blank),
        )
        .expect("open the window");
        cx.activate(true);
        if let Some(after) = quit_after {
            cx.spawn(async move |cx| {
                cx.background_executor().timer(after).await;
                cx.update(|cx| cx.quit());
            })
            .detach();
        }
    });
}
