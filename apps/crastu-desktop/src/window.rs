//! The placeholder window.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use gpui::{
    App, Bounds, Context, IntoElement, Render, TitlebarOptions, Window, WindowBounds,
    WindowOptions, div, prelude::*, px, rgb, size,
};
use gpui_platform::application;

/// Set by the first `render`, so that `--smoke` can tell a window that drew from one that only
/// opened.
static DREW: AtomicBool = AtomicBool::new(false);

/// Set when closing the last window made the application quit.
static QUIT_ON_CLOSE: AtomicBool = AtomicBool::new(false);

/// The Graphite background from `docs/design/tokens.json`. Replaced by the token module of
/// `crastu-ui` (issue "design tokens"); the lint against literals starts there.
const BACKGROUND: u32 = 0x111214;

struct Blank;

impl Render for Blank {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        DREW.store(true, Ordering::Relaxed);
        div().size_full().bg(rgb(BACKGROUND))
    }
}

/// Opens the window and runs until it is closed, or until `quit_after` has passed. Closing the
/// last window quits the application (on macOS GPUI would otherwise keep running without one).
///
/// Fails when the window cannot be opened (no GPU or Metal device, a remote session), and, in
/// the timed mode used for checks, when the window never drew a frame before the time was up.
pub fn open(quit_after: Option<Duration>) -> Result<(), String> {
    let failure: Rc<RefCell<Option<String>>> = Rc::default();
    let report = failure.clone();
    application().run(move |cx: &mut App| {
        crastu_ui::init(cx);
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                QUIT_ON_CLOSE.store(true, Ordering::Relaxed);
                cx.quit();
            }
        })
        .detach();
        let bounds = Bounds::centered(None, size(px(1280.0), px(800.0)), cx);
        let opened = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("Crastu".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_, cx| cx.new(|_| Blank),
        );
        if let Err(e) = opened {
            *report.borrow_mut() = Some(format!("cannot open the window: {e:#}"));
            cx.quit();
            return;
        }
        cx.activate(true);
        if let Some(after) = quit_after {
            // The timed mode closes the window itself, which exercises quit-on-close; the second
            // timer only fires if that did not end the application.
            cx.spawn(async move |cx| {
                cx.background_executor().timer(after).await;
                cx.update(|cx| {
                    for window in cx.windows() {
                        let _ = window.update(cx, |_, window, _| window.remove_window());
                    }
                });
                cx.background_executor().timer(Duration::from_secs(2)).await;
                cx.update(|cx| cx.quit());
            })
            .detach();
        }
    });
    if let Some(message) = failure.borrow_mut().take() {
        return Err(message);
    }
    if quit_after.is_some() {
        if !DREW.load(Ordering::Relaxed) {
            return Err("the window opened but never drew a frame".into());
        }
        if !QUIT_ON_CLOSE.load(Ordering::Relaxed) {
            return Err("closing the last window did not quit the application".into());
        }
    }
    Ok(())
}
