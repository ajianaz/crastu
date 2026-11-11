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

/// Accessibility, Input Monitoring and Screen Recording only say whether access is granted.
/// Not granted after this process asked means the user did not allow it.
#[cfg_attr(
    not(target_os = "macos"),
    allow(dead_code, reason = "only the macOS backend reads system answers")
)]
pub(crate) fn preflight_status(granted: bool, asked: bool) -> Status {
    match (granted, asked) {
        (true, _) => Status::Granted,
        (false, true) => Status::Denied,
        (false, false) => Status::Unknown,
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
    fn a_preflight_that_fails_after_asking_is_a_denial() {
        assert_eq!(preflight_status(true, false), Status::Granted);
        assert_eq!(preflight_status(true, true), Status::Granted);
        assert_eq!(preflight_status(false, false), Status::Unknown);
        assert_eq!(preflight_status(false, true), Status::Denied);
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
