//! The macOS implementation. Each `unsafe` block is one call into a system framework.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::Instant;

use block2::RcBlock;
use objc2_application_services::{
    AXIsProcessTrusted, AXIsProcessTrustedWithOptions, kAXTrustedCheckOptionPrompt,
};
use objc2_av_foundation::{AVCaptureDevice, AVMediaTypeAudio};
use objc2_core_foundation::{CFBoolean, CFDictionary, CFString};
use objc2_core_graphics::{
    CGPreflightListenEventAccess, CGPreflightScreenCaptureAccess, CGRequestListenEventAccess,
    CGRequestScreenCaptureAccess,
};

use crate::rules::{RequestStep, microphone_status, preflight_status, request_step, settings_url};
use crate::{Permission, Status};

/// What this process has done for the three permissions that only report granted or not.
#[derive(Default)]
struct Tried {
    asked: HashMap<Permission, Instant>,
    denied: HashSet<Permission>,
}

static TRIED: Mutex<Option<Tried>> = Mutex::new(None);

fn with_tried<R>(f: impl FnOnce(&mut Tried) -> R) -> Option<R> {
    TRIED
        .lock()
        .ok()
        .map(|mut g| f(g.get_or_insert_with(Tried::default)))
}

/// Milliseconds since this process first asked for the permission, if it did.
fn since_first_ask_ms(p: Permission) -> Option<u128> {
    with_tried(|t| t.asked.get(&p).map(|at| at.elapsed().as_millis())).flatten()
}

fn denied(p: Permission) -> bool {
    with_tried(|t| t.denied.contains(&p)).unwrap_or(false)
}

fn microphone_raw() -> isize {
    let Some(media) = (unsafe { AVMediaTypeAudio }) else {
        return 0;
    };
    // SAFETY: a plain class method that reads the authorization state; no prompt.
    unsafe { AVCaptureDevice::authorizationStatusForMediaType(media) }.0
}

fn granted(p: Permission) -> bool {
    match p {
        // SAFETY: a plain status query.
        Permission::Accessibility => unsafe { AXIsProcessTrusted() },
        Permission::InputMonitoring => CGPreflightListenEventAccess(),
        Permission::ScreenAndSystemAudio => CGPreflightScreenCaptureAccess(),
        Permission::Microphone => microphone_raw() == 3,
    }
}

pub fn status(p: Permission) -> Status {
    match p {
        Permission::Microphone => microphone_status(microphone_raw()),
        _ => preflight_status(granted(p), denied(p)),
    }
}

pub fn request(p: Permission, done: Box<dyn FnOnce(Status) + Send + 'static>) {
    if p == Permission::Microphone {
        return request_microphone(done);
    }
    if granted(p) {
        return done(Status::Granted);
    }
    match request_step(since_first_ask_ms(p)) {
        RequestStep::Prompt => {
            match p {
                Permission::Accessibility => {
                    // SAFETY: reading the exported option-key constant.
                    let key = unsafe { kAXTrustedCheckOptionPrompt };
                    let options = CFDictionary::<CFString, CFBoolean>::from_slices(
                        &[key],
                        &[CFBoolean::new(true)],
                    );
                    // SAFETY: the dictionary holds the one documented option, asking for the prompt.
                    unsafe { AXIsProcessTrustedWithOptions(Some(options.as_opaque())) };
                }
                Permission::InputMonitoring => {
                    CGRequestListenEventAccess();
                }
                _ => {
                    CGRequestScreenCaptureAccess();
                }
            }
            with_tried(|t| {
                t.asked.entry(p).or_insert_with(Instant::now);
            });
        }
        RequestStep::OpenSettings => {
            with_tried(|t| t.denied.insert(p));
            // The pane is the way forward; a failure to open it shows when the user presses again.
            let _ = open_settings(p);
        }
    }
    // The user may still be answering: the poll reports the result.
    done(status(p));
}

fn request_microphone(done: Box<dyn FnOnce(Status) + Send + 'static>) {
    let Some(media) = (unsafe { AVMediaTypeAudio }) else {
        return done(Status::Unknown);
    };
    // The handler runs once, on an arbitrary queue, when the user answers.
    let cell = Mutex::new(Some(done));
    let handler = RcBlock::new(move |ok: objc2::runtime::Bool| {
        if let Some(done) = cell.lock().ok().and_then(|mut c| c.take()) {
            done(if ok.as_bool() {
                Status::Granted
            } else {
                Status::Denied
            });
        }
    });
    // SAFETY: the block is copied by the system and stays valid until it has run.
    unsafe { AVCaptureDevice::requestAccessForMediaType_completionHandler(media, &handler) };
}

pub fn open_settings(p: Permission) -> Result<(), String> {
    // Started and left to run, so a slow `open` never blocks the window; a thread reaps it.
    let mut child = std::process::Command::new("open")
        .arg(settings_url(p))
        .spawn()
        .map_err(|e| format!("could not run `open`: {e}"))?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}
