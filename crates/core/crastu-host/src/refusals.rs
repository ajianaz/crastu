//! Refused network requests in the activity log.
//!
//! With Offline on, a tool that wants the network gets a refusal from the [`NetGate`]; the host
//! writes what was refused and which host (nothing else: no path, query or credentials, which
//! the gate never keeps) as an activity entry, so the person can see what an agent tried.
//!
//! This is an API with tests: no production caller gives a host its gate yet (the desktop has no
//! in-process host and the CLI builds its host without one).

use crastu_core::NetGate;
use crastu_store::params;

use crate::{Caller, Host};

/// The agent name used for a refusal that no agent call caused (the app itself).
const APP: &str = "crastu";
/// The tool name used when the refusal is not tied to a tool.
const NO_TOOL: &str = "network";

impl Host {
    /// Hands the host the gate the process shares. From then on every tool call drains the
    /// refusals made while it ran and logs them against that agent and tool. A refusal that the
    /// app itself caused at the same moment on another thread can be logged against the running
    /// call: the gate does not know who asked.
    pub fn set_net_gate(&self, gate: NetGate) {
        *self.gate.lock().unwrap_or_else(|p| p.into_inner()) = Some(gate);
    }

    /// Drains `gate` into the activity log as `crastu` / `network` entries, for refusals that no
    /// agent call caused. Returns how many were written.
    pub fn record_refusals(&self, gate: &NetGate) -> usize {
        self.write_refusals(gate, APP, false, NO_TOOL, "-")
    }

    /// Logs the refusals the gate collected during a call of `tool`.
    pub(crate) fn record_call_refusals(&self, caller: Caller<'_>, tool: &str, risk: &str) {
        let gate = self.gate.lock().unwrap_or_else(|p| p.into_inner()).clone();
        if let Some(gate) = gate {
            self.write_refusals(&gate, caller.name(), caller.is_paired(), tool, risk);
        }
    }

    fn write_refusals(
        &self,
        gate: &NetGate,
        agent: &str,
        paired: bool,
        tool: &str,
        risk: &str,
    ) -> usize {
        let refusals = gate.drain_refusals();
        let store = self.shared.store();
        for r in &refusals {
            let decision = format!("refused: {} from {}", r.what, r.host);
            let _ = store.conn().execute(
                "INSERT INTO activity (agent, tool, risk, decision, outcome, paired)
                 VALUES (?1, ?2, ?3, ?4, 'blocked', ?5)",
                params![agent, tool, risk, decision, paired],
            );
        }
        refusals.len()
    }
}
