//! What the shell needs from the operating system: permission status, requests and the System
//! Settings link, and the agent pairings and approval policy. The desktop app implements it (with
//! `crastu-os-perms` and the host's stores), so the shell stays a UI crate that depends only on
//! `crastu-app`.

use crastu_app::{Permission, PermissionState};

use crate::agents::{AgentsPlatform, AgentsSnapshot, Class, Level, Secret};
use crate::models::{DownloadEvent, ModelsPlatform};

/// Called with the permission state once a request has been answered, possibly from another
/// thread and possibly after a while.
pub type Reporter = Box<dyn FnOnce(PermissionState) + Send + 'static>;

pub trait Platform: AgentsPlatform + ModelsPlatform {
    /// The current state of a permission, without any prompt. `Unknown` when the system does
    /// not say.
    fn permission(&self, p: Permission) -> PermissionState;

    /// Asks for a permission (the system may show its prompt) and reports the answer.
    fn request(&self, p: Permission, report: Reporter);

    /// Opens the System Settings pane of a permission.
    fn open_settings(&self, p: Permission) -> Result<(), String>;
}

/// A platform that knows nothing and can do nothing: for tests and for builds without a
/// permission model.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoPlatform;

const NO_AGENTS: &str = "this platform has no agent pairing";

impl Platform for NoPlatform {
    fn permission(&self, _: Permission) -> PermissionState {
        PermissionState::Unknown
    }

    fn request(&self, _: Permission, report: Reporter) {
        report(PermissionState::Unknown)
    }

    fn open_settings(&self, _: Permission) -> Result<(), String> {
        Err("this platform cannot open the permission settings".into())
    }
}

impl AgentsPlatform for NoPlatform {
    fn list_agents(&self) -> Result<AgentsSnapshot, String> {
        Ok(AgentsSnapshot::default())
    }

    fn pair_agent(&self, _: &str) -> Result<Secret, String> {
        Err(NO_AGENTS.into())
    }

    fn rotate_agent(&self, _: &str) -> Result<Secret, String> {
        Err(NO_AGENTS.into())
    }

    fn revoke_agent(&self, _: &str) -> Result<(), String> {
        Err(NO_AGENTS.into())
    }

    fn reset_policy(&self) -> Result<bool, String> {
        Err(NO_AGENTS.into())
    }

    fn set_default_policy(&self, _: Class, _: Level) -> Result<(), String> {
        Err(NO_AGENTS.into())
    }

    fn set_agent_override(&self, _: &str, _: Class, _: Option<Level>) -> Result<(), String> {
        Err(NO_AGENTS.into())
    }
}

const NO_MODELS: &str = "this platform has no model downloads";

impl ModelsPlatform for NoPlatform {
    fn installed_models(&self) -> Result<Vec<String>, String> {
        Ok(Vec::new())
    }

    fn start_download(&self, _: &str) -> Result<(), String> {
        Err(NO_MODELS.into())
    }

    fn cancel_download(&self, _: &str) {}

    fn remove_model(&self, _: &str) -> Result<(), String> {
        Err(NO_MODELS.into())
    }

    fn download_events(&self) -> Vec<DownloadEvent> {
        Vec::new()
    }

    fn is_offline(&self) -> bool {
        false
    }
}
