//! The macOS implementation. Each `unsafe` block is one call into a system framework.

use std::collections::HashSet;
use std::sync::Mutex;

use block2::RcBlock;
use objc2_application_services::{AXIsProcessTrusted, AXIsProcessTrustedWithOptions};
use objc2_av_foundation::{AVCaptureDevice, AVMediaTypeAudio};
use objc2_core_foundation::{CFBoolean, CFDictionary, CFString};
use objc2_core_graphics::{
    CGPreflightListenEventAccess, CGPreflightScreenCaptureAccess, CGRequestListenEventAccess,
    CGRequestScreenCaptureAccess,
};

use crate::rules::{microphone_status, preflight_status, settings_url};
use crate::{Permission, Status};

/// Permissions this process has asked for (their preflight can only say granted or not).
static ASKED: Mutex<Option<HashSet<Permission>>> = Mutex::new(None);

fn asked(p: Permission) -> bool {
    ASKED
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|s| s.contains(&p)))
        .unwrap_or(false)
}

fn mark_asked(p: Permission) {
    if let Ok(mut g) = ASKED.lock() {
        g.get_or_insert_with(HashSet::new).insert(p);
    }
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
        _ => preflight_status(granted(p), asked(p)),
    }
}

pub fn request(p: Permission, done: Box<dyn FnOnce(Status) + Send + 'static>) {
    match p {
        Permission::Microphone => {
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
            unsafe {
                AVCaptureDevice::requestAccessForMediaType_completionHandler(media, &handler)
            };
        }
        Permission::Accessibility => {
            let key = CFString::from_static_str("AXTrustedCheckOptionPrompt");
            let options =
                CFDictionary::<CFString, CFBoolean>::from_slices(&[&key], &[CFBoolean::new(true)]);
            // SAFETY: the dictionary holds the one documented option, asking for the prompt.
            unsafe { AXIsProcessTrustedWithOptions(Some(options.as_opaque())) };
            mark_asked(p);
            done(status(p));
        }
        Permission::InputMonitoring => {
            CGRequestListenEventAccess();
            mark_asked(p);
            done(status(p));
        }
        Permission::ScreenAndSystemAudio => {
            CGRequestScreenCaptureAccess();
            mark_asked(p);
            done(status(p));
        }
    }
}

pub fn open_settings(p: Permission) -> Result<(), String> {
    std::process::Command::new("open")
        .arg(settings_url(p))
        .status()
        .map_err(|e| format!("could not run `open`: {e}"))
        .and_then(|s| {
            if s.success() {
                Ok(())
            } else {
                Err(format!("`open` failed: {s}"))
            }
        })
}
