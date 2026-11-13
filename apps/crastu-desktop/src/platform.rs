//! The operating system as the shell sees it: permissions through `crastu-os-perms`.

use crastu_app::{NetGate, Permission, PermissionState};
use crastu_os_perms as os;
use crastu_shell::agents::{AgentsPlatform, AgentsSnapshot, Class, Level, Secret};
use crastu_shell::models::{DownloadEvent, ModelsPlatform};
use crastu_shell::platform::{Platform, Reporter};

use crate::agents::AgentStore;
use crate::models::ModelStore;

pub struct Desktop {
    gate: NetGate,
    /// The models folder and the downloads; an error when the per-user folder is unknown.
    models: Result<ModelStore, String>,
}

impl Desktop {
    /// `gate` is the process's Offline gate: the downloads ask it before every request.
    pub fn new(gate: NetGate) -> Desktop {
        let models = crastu_settings::data_dir().map(|dir| {
            ModelStore::new(
                dir.join("models"),
                crastu_models::catalogue(),
                gate.clone(),
                std::sync::Arc::new(|| Box::new(crastu_models_http::HttpFetch::new())),
            )
        });
        Desktop { gate, models }
    }

    fn store(&self) -> Result<&ModelStore, String> {
        self.models.as_ref().map_err(|e| e.clone())
    }
}

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

    fn reset_policy(&self) -> Result<bool, String> {
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

impl ModelsPlatform for Desktop {
    fn installed_models(&self) -> Result<Vec<String>, String> {
        Ok(self.store()?.installed())
    }

    fn start_download(&self, id: &str) -> Result<(), String> {
        self.store()?.start(id)
    }

    fn cancel_download(&self, id: &str) {
        if let Ok(store) = self.store() {
            store.cancel(id);
        }
    }

    fn remove_model(&self, id: &str) -> Result<(), String> {
        self.store()?.remove(id)
    }

    fn download_events(&self) -> Vec<DownloadEvent> {
        self.store().map(ModelStore::events).unwrap_or_default()
    }

    fn is_offline(&self) -> bool {
        self.gate.is_offline()
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
