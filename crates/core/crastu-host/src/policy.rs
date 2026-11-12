use std::collections::HashMap;

use crastu_module::Risk;
use serde_json::Value;

/// What happens when an agent calls a tool of a given risk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Auto,
    Ask,
    Deny,
}

/// The approval policy from Settings, Agents. Reads are always automatic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Policy {
    pub change: Decision,
    pub delete: Decision,
    per_agent: HashMap<(String, Risk), Decision>,
}

impl Default for Policy {
    fn default() -> Self {
        Policy {
            change: Decision::Ask,
            delete: Decision::Ask,
            per_agent: HashMap::new(),
        }
    }
}

fn strictness(d: Decision) -> u8 {
    match d {
        Decision::Auto => 0,
        Decision::Ask => 1,
        Decision::Deny => 2,
    }
}

impl Policy {
    /// Overrides the decision for one agent. Read cannot be restricted below Auto, and Change
    /// cannot be set to Deny (the design offers Ask or Auto for it).
    pub fn set_agent(&mut self, agent: &str, risk: Risk, decision: Decision) {
        if risk == Risk::Read || (risk == Risk::Change && decision == Decision::Deny) {
            return;
        }
        self.per_agent.insert((agent.to_string(), risk), decision);
    }

    /// The decision for a caller without a valid token that declares the name `declared`: the
    /// defaults, or an override stored under that name when it is stricter (Deny, then Ask, then
    /// Auto). An override can only restrict an unpaired caller, never grant it more: otherwise
    /// a Deny would hold only while the agent shows its token.
    pub fn decide_unpaired(&self, declared: &str, risk: Risk) -> Decision {
        let default = match risk {
            Risk::Read => return Decision::Auto,
            Risk::Change => self.change,
            Risk::Delete => self.delete,
        };
        match self.per_agent.get(&(declared.to_string(), risk)) {
            Some(o) if strictness(*o) > strictness(default) => *o,
            _ => default,
        }
    }

    /// The decision for a paired agent: its override if it has one, else the defaults.
    pub fn decide(&self, agent: &str, risk: Risk) -> Decision {
        if risk == Risk::Read {
            return Decision::Auto;
        }
        if let Some(d) = self.per_agent.get(&(agent.to_string(), risk)) {
            return *d;
        }
        match risk {
            Risk::Read => Decision::Auto,
            Risk::Change => self.change,
            Risk::Delete => self.delete,
        }
    }
}

#[derive(Debug)]
pub struct ApprovalRequest<'a> {
    /// The paired agent's name, or the name an unpaired client declared.
    pub agent: &'a str,
    /// Whether a valid token backed the call; an unpaired name is only a label.
    pub paired: bool,
    pub tool: &'a str,
    pub risk: Risk,
    pub args: &'a Value,
}

/// Answers Ask decisions. The GUI shows the approval dialog; a headless host has no one to ask.
pub trait Approver: Send + Sync {
    fn approve(&self, request: &ApprovalRequest<'_>) -> bool;
}

/// For a host without a window: every Ask is refused with a message pointing to the app.
#[derive(Debug, Default, Clone, Copy)]
pub struct DenyAsk;

impl Approver for DenyAsk {
    fn approve(&self, _request: &ApprovalRequest<'_>) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_guard_rails() {
        let mut p = Policy::default();
        assert_eq!(p.decide("a", Risk::Read), Decision::Auto);
        assert_eq!(p.decide("a", Risk::Change), Decision::Ask);
        assert_eq!(p.decide("a", Risk::Delete), Decision::Ask);
        p.set_agent("a", Risk::Read, Decision::Deny);
        p.set_agent("a", Risk::Change, Decision::Deny);
        assert_eq!(p.decide("a", Risk::Read), Decision::Auto);
        assert_eq!(p.decide("a", Risk::Change), Decision::Ask);
    }
}
