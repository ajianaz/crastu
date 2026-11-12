//! The operating system as the shell sees it: permissions through `crastu-os-perms`.

use crastu_app::{Permission, PermissionState};
use crastu_os_perms as os;
use crastu_shell::agents::{AgentsPlatform, AgentsSnapshot, Class, Level, Secret};
use crastu_shell::platform::{Platform, Reporter};

use crate::agents::AgentStore;

pub struct Desktop;

fn to_os(p: Permission) -> os::Permission {
    match p {
        Permission::Microphone => os::Permission::Microphone,
        Permission::Accessibility => os::Permission::Accessibility,
        Permission::InputMonitoring => os::Permission::InputMonitoring,
        Permission::ScreenAndSystemAudio => os::Permission::ScreenAndSystemAudio,
    }
}

fn to_app(s: os::Status) -> PermissionState {
    match s {
        os::Status::Granted => PermissionState::Granted,
        os::Status::Denied => PermissionState::Denied,
        os::Status::Unknown => PermissionState::Unknown,
    }
}

impl Platform for Desktop {
    fn permission(&self, p: Permission) -> PermissionState {
        to_app(os::status(to_os(p)))
    }

    fn request(&self, p: Permission, report: Reporter) {
        os::request(to_os(p), Box::new(move |s| report(to_app(s))));
    }

    fn open_settings(&self, p: Permission) -> Result<(), String> {
        os::open_settings(to_os(p))
    }
}

impl AgentsPlatform for Desktop {
    fn list_agents(&self) -> Result<AgentsSnapshot, String> {
        AgentStore::per_user()?.snapshot()
    }

    fn pair_agent(&self, name: &str) -> Result<Secret, String> {
        AgentStore::per_user()?.pair(name)
    }

    fn rotate_agent(&self, name: &str) -> Result<Secret, String> {
        AgentStore::per_user()?.rotate(name)
    }

    fn revoke_agent(&self, name: &str) -> Result<(), String> {
        AgentStore::per_user()?.revoke(name)
    }

    fn reset_policy(&self) -> Result<(), String> {
        AgentStore::per_user()?.reset_policy()
    }

    fn set_default_policy(&self, class: Class, level: Level) -> Result<(), String> {
        AgentStore::per_user()?.set_default(class, level)
    }

    fn set_agent_override(
        &self,
        agent: &str,
        class: Class,
        level: Option<Level>,
    ) -> Result<(), String> {
        AgentStore::per_user()?.set_override(agent, class, level)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_permissions_and_states_map_one_to_one() {
        for (app, os_p) in [
            (Permission::Microphone, os::Permission::Microphone),
            (Permission::Accessibility, os::Permission::Accessibility),
            (Permission::InputMonitoring, os::Permission::InputMonitoring),
            (
                Permission::ScreenAndSystemAudio,
                os::Permission::ScreenAndSystemAudio,
            ),
        ] {
            assert_eq!(to_os(app), os_p);
        }
        assert_eq!(to_app(os::Status::Granted), PermissionState::Granted);
        assert_eq!(to_app(os::Status::Denied), PermissionState::Denied);
        assert_eq!(to_app(os::Status::Unknown), PermissionState::Unknown);
        assert_eq!(Permission::ALL.len(), os::Permission::ALL.len());
    }
}
