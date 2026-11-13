//! The operating system as the shell sees it: permissions through `crastu-os-perms`.

use crastu_app::{NetGate, Permission, PermissionState};
use crastu_diagnostics::{Diagnostics, build};
use crastu_os_perms as os;
use crastu_recorder::Recorder;
use crastu_shell::about::{AboutInfo, AboutPlatform, open_command, os_label};
use crastu_shell::agents::{AgentsPlatform, AgentsSnapshot, Class, Level, Secret};
use crastu_shell::models::{DownloadEvent, ModelsPlatform};
use crastu_shell::platform::{Platform, Reporter};

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::agents::AgentStore;
use crate::models::ModelStore;

/// The platform of the window. It holds the Offline gate and the models folder, and the flight
/// recorder of this run for About.
pub struct Desktop {
    gate: NetGate,
    /// The models folder and the downloads; an error when the per-user folder is unknown.
    models: Result<ModelStore, String>,
    pub recorder: Recorder,
}

impl Desktop {
    /// `gate` is the process's Offline gate: the downloads ask it before every request.
    pub fn new(gate: NetGate, recorder: Recorder) -> Desktop {
        let models = crastu_settings::data_dir().map(|dir| {
            ModelStore::new(
                dir.join("models"),
                crastu_models::catalogue(),
                gate.clone(),
                std::sync::Arc::new(|| Box::new(crastu_models_http::HttpFetch::new())),
            )
        });
        Desktop {
            gate,
            models,
            recorder,
        }
    }

    fn store(&self) -> Result<&ModelStore, String> {
        self.models.as_ref().map_err(|e| e.clone())
    }

    /// The folder with the logs of every app: the parent of this app's folder.
    fn log_root(&self) -> Option<PathBuf> {
        self.recorder
            .session_dir()?
            .parent()?
            .parent()
            .map(Path::to_path_buf)
    }
}

/// Starts the file manager on `folder` and returns without waiting for it. The program and the
/// path are separate arguments (no shell); a thread reaps the child so it does not stay a zombie.
fn reveal(folder: &Path) -> Result<(), String> {
    let cmd = open_command(std::env::consts::OS, folder).ok_or("there is no folder to open")?;
    let mut child = Command::new(cmd.program)
        .args(&cmd.args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("{} did not start: {e}", cmd.program))?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
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

impl AboutPlatform for Desktop {
    fn about(&self) -> AboutInfo {
        AboutInfo {
            version: env!("CARGO_PKG_VERSION").to_string(),
            os: os_label(std::env::consts::OS, std::env::consts::ARCH),
            log_dir: self.log_root().map(|p| p.display().to_string()),
        }
    }

    fn open_logs(&self) -> Result<(), String> {
        let root = self.log_root().ok_or("this run has no log folder")?;
        reveal(&root)
    }

    fn diagnostics(&self) -> Result<String, String> {
        let current = self.recorder.session_dir();
        let modules = crate::built_in_modules();
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .ok();
        Ok(build(&Diagnostics {
            app: crate::recording::APP,
            version: env!("CARGO_PKG_VERSION"),
            os: &os_label(std::env::consts::OS, std::env::consts::ARCH),
            modules: &modules,
            current,
            app_dir: current.and_then(Path::parent),
            home: home.as_deref(),
        }))
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
    fn about_names_the_log_root_and_the_diagnostics_come_from_this_run() {
        let root = tempfile::tempdir().unwrap();
        let recorder = Recorder::start(crate::recording::APP, "0", Some(root.path()));
        recorder.info("test", "hello from the window");
        let desktop = Desktop::new(NetGate::default(), recorder);
        let info = desktop.about();
        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(info.log_dir, Some(root.path().display().to_string()));
        let text = desktop.diagnostics().unwrap();
        assert!(text.contains("app: crastu-desktop"), "{text}");
        assert!(text.contains("hello from the window"), "{text}");
    }

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
