//! The main window: the shell, opened with the saved settings.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crastu_app::Settings;
use crastu_shell::Shell;
use gpui::{
    App, Bounds, Context, Entity, IntoElement, Render, TitlebarOptions, Window, WindowBounds,
    WindowOptions, div, prelude::*, px, size,
};
use gpui_platform::application;

use crastu_recorder::Frames;

/// Set by the first `render`, so that `--smoke` can tell a window that drew from one that only
/// opened.
static DREW: AtomicBool = AtomicBool::new(false);

/// The flight recorder of this run, for `fail`, which ends the process without returning.
static RECORDER: OnceLock<crastu_recorder::Recorder> = OnceLock::new();

/// The root view: it notes that a frame was drawn (for `--smoke` and for the frame times in
/// `perf.jsonl`) and shows the shell. The root view is rendered on every draw, so the time
/// between two calls is the time between two drawn frames. Needs hands-on: the number it gives
/// on real hardware has not been compared with another tool.
struct Frame {
    shell: Entity<Shell>,
    frames: Frames,
}

impl Render for Frame {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        DREW.store(true, Ordering::Relaxed);
        self.frames.mark();
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
    let downloads_gate = gate.clone();
    let settings_unreadable = loaded.is_err();
    let settings = loaded.unwrap_or_else(|_| Settings::in_memory());
    // The panic hook and the crash handler are installed here, on the main thread, before any
    // window exists.
    let recorder = RECORDER.get_or_init(crate::recording::start).clone();
    let frames = Frames::new();
    application()
        .with_assets(crastu_ui::Assets)
        .run(move |cx: &mut App| {
            crastu_ui::init(cx);
            crastu_shell::init(cx);
            // `perf.jsonl`: frame times and memory once a second, until the application quits.
            let (rec, tick) = (recorder.clone(), frames.clone());
            cx.spawn(async move |cx| {
                loop {
                    cx.background_executor().timer(Duration::from_secs(1)).await;
                    rec.perf(&tick.sample());
                }
            })
            .detach();
            // Quitting from the menu or by closing the last window ends the run cleanly. The
            // process can end inside `quit` on macOS, so this is the place for it.
            let ending = recorder.clone();
            cx.on_app_quit(move |_| {
                ending.info("desktop", "quitting");
                ending.finish(crastu_recorder::Outcome::Ok);
                async {}
            })
            .detach();
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
                            std::rc::Rc::new(crate::platform::Desktop::new(
                                downloads_gate,
                                recorder,
                            )),
                            window,
                            cx,
                        )
                    });
                    cx.new(|_| Frame { shell, frames })
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
    if let Some(r) = RECORDER.get() {
        r.error("desktop", message);
        r.finish(crastu_recorder::Outcome::Error);
    }
    std::process::exit(1)
}
