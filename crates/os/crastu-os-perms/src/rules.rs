//! The platform-independent rules: how a system answer becomes a [`Status`], and where System
//! Settings keeps each permission.

use crate::{Permission, Status};

/// The macOS authorization status of the microphone (`AVAuthorizationStatus`):
/// 0 not determined, 1 restricted, 2 denied, 3 authorized.
#[cfg_attr(
    not(target_os = "macos"),
    allow(dead_code, reason = "only the macOS backend reads system answers")
)]
pub(crate) fn microphone_status(raw: isize) -> Status {
    match raw {
        3 => Status::Granted,
        1 | 2 => Status::Denied,
        _ => Status::Unknown,
    }
}

/// Accessibility, Input Monitoring and Screen Recording only say whether access is granted, and
/// the system never says that the user refused. So a request that did not end in a grant is not
/// called a denial at once (the user may still be answering): `denied` is set only when the user
/// pressed Allow again and it was still not granted (see [`request_step`]).
#[cfg_attr(
    not(target_os = "macos"),
    allow(dead_code, reason = "only the macOS backend reads system answers")
)]
pub(crate) fn preflight_status(granted: bool, denied: bool) -> Status {
    match (granted, denied) {
        (true, _) => Status::Granted,
        (false, true) => Status::Denied,
        (false, false) => Status::Unknown,
    }
}

/// What pressing Allow does for those three permissions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RequestStep {
    /// The first time: show the system prompt.
    Prompt,
    /// Already asked and still not granted: the prompt will not help, send the user to System
    /// Settings and treat it as a denial.
    OpenSettings,
}

/// A second press within this time of the first is the same click, not a new attempt: the system
/// prompt is probably still open.
#[cfg_attr(
    not(target_os = "macos"),
    allow(dead_code, reason = "only the macOS backend uses it")
)]
pub(crate) const SAME_ATTEMPT_MS: u128 = 3_000;

#[cfg_attr(
    not(target_os = "macos"),
    allow(dead_code, reason = "only the macOS backend uses it")
)]
pub(crate) fn request_step(since_first_ask_ms: Option<u128>) -> RequestStep {
    match since_first_ask_ms {
        Some(ms) if ms >= SAME_ATTEMPT_MS => RequestStep::OpenSettings,
        // Never asked, or asked a moment ago: (re)show the prompt, which is harmless.
        _ => RequestStep::Prompt,
    }
}

/// The System Settings link of a permission (`x-apple.systempreferences` URL).
pub fn settings_url(p: Permission) -> &'static str {
    match p {
        Permission::Microphone => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
        }
        Permission::Accessibility => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
        }
        Permission::InputMonitoring => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent"
        }
        Permission::ScreenAndSystemAudio => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_microphone_status_follows_the_system_values() {
        assert_eq!(microphone_status(0), Status::Unknown);
        assert_eq!(microphone_status(1), Status::Denied);
        assert_eq!(microphone_status(2), Status::Denied);
        assert_eq!(microphone_status(3), Status::Granted);
        assert_eq!(microphone_status(99), Status::Unknown);
    }

    #[test]
    fn a_preflight_is_a_denial_only_once_the_user_asked_twice() {
        assert_eq!(preflight_status(true, false), Status::Granted);
        assert_eq!(preflight_status(true, true), Status::Granted);
        assert_eq!(preflight_status(false, false), Status::Unknown);
        assert_eq!(preflight_status(false, true), Status::Denied);
        assert_eq!(request_step(None), RequestStep::Prompt);
        assert_eq!(
            request_step(Some(200)),
            RequestStep::Prompt,
            "a double click"
        );
        assert_eq!(
            request_step(Some(SAME_ATTEMPT_MS)),
            RequestStep::OpenSettings
        );
    }

    #[test]
    fn every_permission_has_its_own_settings_pane() {
        let urls: Vec<_> = Permission::ALL.iter().map(|p| settings_url(*p)).collect();
        for u in &urls {
            assert!(
                u.starts_with("x-apple.systempreferences:com.apple.preference.security?Privacy_")
            );
        }
        let mut sorted = urls.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 4);
    }
}
