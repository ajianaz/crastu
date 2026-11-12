//! The component gallery: every component in every state, in the chosen theme, font and density.
//!
//! ```text
//! crastu-gallery            open the window
//! crastu-gallery --smoke    open it, draw, and quit after 1.5 seconds (for checks)
//! ```
//!
//! Compare it with the Components pages of `docs/design/` (see the README there).

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod app;
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod core_page;
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod feedback_page;

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn run() -> std::process::ExitCode {
    use std::time::Duration;

    use gpui::{App, Bounds, WindowBounds, WindowOptions, prelude::*, px, size};
    use gpui_platform::application;

    let smoke = std::env::args().any(|a| a == "--smoke");
    application()
        .with_assets(crastu_ui::Assets)
        .run(move |cx: &mut App| {
            crastu_ui::init(cx);
            let bounds = Bounds::centered(None, size(px(1180.0), px(860.0)), cx);
            let opened = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |_, cx| cx.new(app::Gallery::new),
            );
            if opened.is_err() {
                // Quitting ends the process inside `run` on macOS, so the failure is reported here.
                eprintln!("crastu-gallery: cannot open the window");
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
    std::process::ExitCode::SUCCESS
}

fn main() -> std::process::ExitCode {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    return run();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        eprintln!("crastu-gallery needs macOS or Windows");
        std::process::ExitCode::FAILURE
    }
}
