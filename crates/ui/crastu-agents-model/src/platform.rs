//! What Settings, Agents needs from the app: the agent pairings and the approval policy. The
//! desktop app implements it over the host's stores; `crastu-shell-model`'s `Platform` includes it.

use crate::{AgentsSnapshot, Class, Level, Secret};

pub trait AgentsPlatform {
    /// The paired agents, the default policy and the newest calls. Never a token.
    fn list_agents(&self) -> Result<AgentsSnapshot, String>;

    /// Pairs a new agent and returns its token, to be shown once. The name follows the host's
    /// rules (1 to 64 of a-z, 0-9, '-', '_', '.').
    fn pair_agent(&self, name: &str) -> Result<Secret, String>;

    /// Gives an agent a new token; the old one stops working.
    fn rotate_agent(&self, name: &str) -> Result<Secret, String>;

    /// Stops an agent's token from working.
    fn revoke_agent(&self, name: &str) -> Result<(), String>;

    /// Replaces the whole stored policy with the defaults (Ask for changes and deletions, no
    /// overrides): the way out of a damaged policy.
    /// Returns false, and changes nothing, when the policy is no longer damaged.
    fn reset_policy(&self) -> Result<bool, String>;

    /// Sets the default decision of a class of call.
    fn set_default_policy(&self, class: Class, level: Level) -> Result<(), String>;

    /// Sets (`Some`) or clears (`None`) one agent's decision for a class of call.
    fn set_agent_override(
        &self,
        agent: &str,
        class: Class,
        level: Option<Level>,
    ) -> Result<(), String>;
}
