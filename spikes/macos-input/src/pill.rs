//! Dictation pill test: can a small floating window show while the user types in another app
//! without taking focus, and does its animation keep running when the app's main thread is busy?
//!
//! ```text
//! pill [--seconds N] [--delay N] [--busy] [--activating]
//! ```
//!
//! Default: a borderless non-activating panel (`NSWindowStyleMaskNonactivatingPanel`) at the
//! bottom centre of the main screen with five bars animated by Core Animation, which runs in
//! the render server and does not depend on this process's main thread. `--busy` blocks the
//! main thread for 150 ms every second to show exactly that. `--activating` is the control: an
//! ordinary key window that activates the app. It must steal focus; if the measurement did not
//! see that, the measurement would be worthless.
//!
//! Before showing, wait `--delay` seconds (default 4) so you can click into a text field of the
//! app under test and keep typing. The frontmost app is sampled before, while and after the
//! pill is shown; the verdict says whether it ever changed.

use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSColor, NSPanel, NSScreen,
    NSView, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_core_foundation::{CGFloat, CGPoint, CGRect, CGSize};
use objc2_foundation::{NSNumber, NSPoint, NSRect, NSSize, NSString};
use objc2_quartz_core::{CABasicAnimation, CALayer, CAMediaTiming, CATransaction};

use super::{Target, frontmost, log, spin};

/// `NSStatusWindowLevel`: above normal and floating windows, below menus.
const STATUS_LEVEL: isize = 25;
const WIDTH: f64 = 168.0;
const HEIGHT: f64 = 44.0;
const BARS: usize = 5;

struct Options {
    seconds: u64,
    delay: u64,
    busy: bool,
    activating: bool,
}

fn parse(args: &[String]) -> Result<Options> {
    let mut o = Options {
        seconds: 8,
        delay: 4,
        busy: false,
        activating: false,
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--seconds" => o.seconds = it.next().ok_or_else(|| anyhow!("--seconds N"))?.parse()?,
            "--delay" => o.delay = it.next().ok_or_else(|| anyhow!("--delay N"))?.parse()?,
            "--busy" => o.busy = true,
            "--activating" => o.activating = true,
            other => return Err(anyhow!("unknown option {other}")),
        }
    }
    Ok(o)
}

/// Five bars that scale up and down at different speeds, forever, as Core Animation animations.
fn add_bars(host: &CALayer) {
    let bar_w = 6.0;
    let gap = 8.0;
    let total = BARS as CGFloat * bar_w + (BARS as CGFloat - 1.0) * gap;
    let x0 = (WIDTH - total) / 2.0;
    for i in 0..BARS {
        let bar = CALayer::new();
        let h = 22.0;
        bar.setBounds(CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(bar_w, h)));
        bar.setPosition(CGPoint::new(
            x0 + i as CGFloat * (bar_w + gap) + bar_w / 2.0,
            HEIGHT / 2.0,
        ));
        bar.setCornerRadius(bar_w / 2.0);
        bar.setBackgroundColor(Some(&color_ref(0.30, 0.76, 0.69, 1.0)));
        let anim =
            CABasicAnimation::animationWithKeyPath(Some(&NSString::from_str("transform.scale.y")));
        let from = NSNumber::new_f64(0.25);
        let to = NSNumber::new_f64(1.0);
        let (from, to): (&AnyObject, &AnyObject) = (&from, &to);
        // SAFETY: the key path `transform.scale.y` takes NSNumber values.
        unsafe {
            anim.setFromValue(Some(from));
            anim.setToValue(Some(to));
        }
        anim.setDuration(0.45 + 0.09 * i as f64);
        anim.setAutoreverses(true);
        anim.setRepeatCount(f32::INFINITY);
        bar.addAnimation_forKey(&anim, Some(&NSString::from_str("pulse")));
        host.addSublayer(&bar);
    }
}

fn color_ref(
    r: f64,
    g: f64,
    b: f64,
    a: f64,
) -> objc2_core_foundation::CFRetained<objc2_core_graphics::CGColor> {
    NSColor::colorWithSRGBRed_green_blue_alpha(r, g, b, a)
        .CGColor()
        .into()
}

fn build_panel(mtm: MainThreadMarker, activating: bool) -> Result<Retained<NSPanel>> {
    let screen = NSScreen::mainScreen(mtm).ok_or_else(|| anyhow!("no main screen"))?;
    let f = screen.frame();
    let frame = NSRect::new(
        NSPoint::new(f.origin.x + (f.size.width - WIDTH) / 2.0, f.origin.y + 72.0),
        NSSize::new(WIDTH, HEIGHT),
    );
    let style = if activating {
        NSWindowStyleMask::Borderless
    } else {
        NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel
    };
    let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
        NSPanel::alloc(mtm),
        frame,
        style,
        NSBackingStoreType::Buffered,
        false,
    );
    panel.setOpaque(false);
    panel.setBackgroundColor(Some(&NSColor::clearColor()));
    panel.setHasShadow(true);
    panel.setLevel(STATUS_LEVEL);
    panel.setFloatingPanel(true);
    panel.setHidesOnDeactivate(false);
    if !activating {
        // Never become the key window unless a control inside needs the keyboard.
        panel.setBecomesKeyOnlyIfNeeded(true);
        panel.setIgnoresMouseEvents(true);
    }
    panel.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );
    let view = NSView::initWithFrame(
        NSView::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(WIDTH, HEIGHT)),
    );
    view.setWantsLayer(true);
    let layer = view.layer().ok_or_else(|| anyhow!("view has no layer"))?;
    layer.setCornerRadius(HEIGHT / 2.0);
    layer.setBackgroundColor(Some(&color_ref(0.10, 0.11, 0.13, 0.96)));
    CATransaction::begin();
    CATransaction::setDisableActions(true);
    add_bars(&layer);
    CATransaction::commit();
    panel.setContentView(Some(&view));
    Ok(panel)
}

fn describe(t: &Option<Target>) -> String {
    t.as_ref()
        .map(|t| format!("{} (pid {})", t.name, t.pid))
        .unwrap_or_else(|| "none".into())
}

pub fn run(args: &[String]) -> Result<()> {
    let o = parse(args)?;
    let mtm = MainThreadMarker::new().ok_or_else(|| anyhow!("must run on the main thread"))?;
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    log(format!(
        "pill: {} panel; click into a text field of the app under test now, showing in {} s",
        if o.activating {
            "ACTIVATING (control)"
        } else {
            "non-activating"
        },
        o.delay
    ));
    spin(Duration::from_secs(o.delay));

    let before = frontmost();
    let panel = build_panel(mtm, o.activating)?;
    let shown = Instant::now();
    if o.activating {
        panel.makeKeyAndOrderFront(None);
        #[allow(deprecated)]
        app.activate();
    } else {
        panel.orderFrontRegardless();
    }
    log(format!(
        "pill shown; frontmost before: {}; can become key: {}",
        describe(&before),
        panel.canBecomeKeyWindow()
    ));

    let mut changed = false;
    let mut next_sample = Duration::from_millis(100);
    let mut last_busy = Instant::now();
    let mut blocked = 0u32;
    while shown.elapsed() < Duration::from_secs(o.seconds) {
        spin(Duration::from_millis(16));
        if o.busy && last_busy.elapsed() >= Duration::from_secs(1) {
            // Simulate a busy main thread (a transcription step, a database write).
            std::thread::sleep(Duration::from_millis(150));
            blocked += 1;
            last_busy = Instant::now();
        }
        if shown.elapsed() >= next_sample {
            let now = frontmost();
            let same = now.as_ref().map(|t| t.pid) == before.as_ref().map(|t| t.pid);
            changed |= !same;
            log(format!(
                "t+{:>4} ms frontmost: {}{}; app active: {}; panel is key: {}",
                shown.elapsed().as_millis(),
                describe(&now),
                if same { "" } else { "   <-- CHANGED" },
                app.isActive(),
                panel.isKeyWindow()
            ));
            next_sample += Duration::from_millis(1000);
        }
    }
    panel.orderOut(None);
    spin(Duration::from_millis(200));
    let after = frontmost();
    let same_after = after.as_ref().map(|t| t.pid) == before.as_ref().map(|t| t.pid);
    log(format!(
        "pill hidden; frontmost after: {}{}",
        describe(&after),
        if same_after { "" } else { "   <-- CHANGED" }
    ));
    log(format!(
        "pill verdict ({}): focus {} for the whole run; main thread blocked {blocked} time(s)",
        if o.activating {
            "control"
        } else {
            "non-activating"
        },
        if changed || !same_after {
            "WAS TAKEN"
        } else {
            "stayed with the original app"
        }
    ));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn defaults_and_flags() {
        let o = parse(&[]).unwrap();
        assert_eq!(
            (o.seconds, o.delay, o.busy, o.activating),
            (8, 4, false, false)
        );
        let o = parse(&args(&[
            "--seconds",
            "20",
            "--delay",
            "1",
            "--busy",
            "--activating",
        ]))
        .unwrap();
        assert_eq!(
            (o.seconds, o.delay, o.busy, o.activating),
            (20, 1, true, true)
        );
    }

    #[test]
    fn rejects_unknown_and_incomplete_options() {
        assert!(parse(&args(&["--nope"])).is_err());
        assert!(parse(&args(&["--seconds"])).is_err());
        assert!(parse(&args(&["--seconds", "many"])).is_err());
    }
}
