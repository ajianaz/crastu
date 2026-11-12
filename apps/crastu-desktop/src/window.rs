//! The main window: the shell, opened with the saved settings.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crastu_app::Settings;
use crastu_shell::Shell;
use gpui::{
    App, Bounds, Context, Entity, IntoElement, Render, TitlebarOptions, Window, WindowBounds,
    WindowOptions, div, prelude::*, px, size,
};
use gpui_platform::application;

/// Set by the first `render`, so that `--smoke` can tell a window that drew from one that only
/// opened.
static DREW: AtomicBool = AtomicBool::new(false);

/// The root view: it only notes that a frame was drawn (for `--smoke`) and shows the shell.
struct Frame {
    shell: Entity<Shell>,
}

impl Render for Frame {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        DREW.store(true, Ordering::Relaxed);
        div().size_full().child(self.shell.clone())
    }
}

/// Opens the window and runs until it is closed, or until `quit_after` has passed. Closing the
/// last window quits the application (on macOS GPUI would otherwise keep running without one).
///
/// Exits with a failure code when the window cannot be opened (no GPU or Metal device, a remote session), and, in
/// the timed mode used for checks, when the window never drew a frame or closing it did not quit.
///
/// On macOS quitting ends the process inside `run` (`NSApplication terminate:`), so code after
/// `run` does not execute there. Every failure is therefore reported from inside the application
/// with [`fail`], which prints and exits with a non-zero code on its own.
pub fn open(quit_after: Option<Duration>) {
    // The timed check must not touch the user's settings. A settings file that cannot be read
    // (corrupt, no per-user folder) does not stop the app: it starts with the defaults for this
    // session, leaves the file as it is and says so, and Offline is on.
    let loaded = if quit_after.is_some() {
        Ok(Settings::in_memory())
    } else {
        crate::gate::load_settings()
    };
    let env = std::env::var(crastu_settings::OFFLINE_ENV).ok();
    let (gate, locked_by_env, notice) = crate::gate::desktop_gate(&loaded, env.as_deref());
    let settings_unreadable = loaded.is_err();
    let settings = loaded.unwrap_or_else(|_| Settings::in_memory());
    application()
        .with_assets(crastu_ui::Assets)
        .run(move |cx: &mut App| {
            crastu_ui::init(cx);
            crastu_shell::init(cx);
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
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
                move |window, cx| {
                    let shell = cx.new(|cx| {
                        Shell::new(
                            settings,
                            crastu_shell::NetStart {
                                gate,
                                locked_by_env,
                                settings_unreadable,
                            },
                            notice,
                            std::rc::Rc::new(crate::platform::Desktop),
                            window,
                            cx,
                        )
                    });
                    cx.new(|_| Frame { shell })
                },
            );
            if let Err(e) = opened {
                fail(&format!("cannot open the window: {e:#}"));
            }
            cx.activate(true);
            if let Some(after) = quit_after {
                // The timed mode checks that a frame was drawn, then closes the window itself, which
                // exercises quit-on-close. The process ends with the last window, so reaching the
                // second timer means it did not.
                cx.spawn(async move |cx| {
                    cx.background_executor().timer(after).await;
                    if !DREW.load(Ordering::Relaxed) {
                        fail("the window opened but never drew a frame");
                    }
                    cx.update(|cx| {
                        for window in cx.windows() {
                            let _ = window.update(cx, |_, window, _| window.remove_window());
                        }
                    });
                    cx.background_executor().timer(Duration::from_secs(2)).await;
                    fail("closing the last window did not quit the application");
                })
                .detach();
            }
        });
}

/// Prints the reason and ends the process with a failure code.
fn fail(message: &str) -> ! {
    eprintln!("crastu: {message}");
    std::process::exit(1)
}
