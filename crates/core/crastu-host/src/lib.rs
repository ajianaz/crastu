//! Headless core process: runs modules, jobs, store and engines without a window.
//!
//! Every tool call from an agent goes through [`Host::call`]: the approval policy decides Auto,
//! Ask or Deny for the tool's risk, an [`Approver`] answers Ask, and the outcome is written to the
//! activity log. The GUI, the CLI and the MCP shim all reach the host the same way, in process or
//! over [`crastu_ipc`].

mod core_module;
pub mod pairing;
#[cfg(test)]
mod pairing_tests;
mod policy;
#[cfg(test)]
mod tests;

use std::sync::{Arc, Mutex, MutexGuard};

use crastu_ipc::{CallParams, ToolInfo, method};
use crastu_jobs::JobQueue;
use crastu_module::{Module, Registry, Risk, ToolError};
use crastu_store::{Migration, Store, params};
use serde_json::{Value, json};

pub use core_module::CoreModule;
pub use policy::{ApprovalRequest, Approver, Decision, DenyAsk, Policy};

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        sql: "CREATE TABLE projects (
              id TEXT PRIMARY KEY,
              name TEXT NOT NULL,
              created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
          );
          CREATE TABLE activity (
              id INTEGER PRIMARY KEY,
              at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
              agent TEXT NOT NULL,
              tool TEXT NOT NULL,
              risk TEXT NOT NULL,
              decision TEXT NOT NULL,
              outcome TEXT NOT NULL
          );",
    },
    Migration {
        version: 2,
        sql: "CREATE TABLE agents (
              name TEXT PRIMARY KEY,
              token_hash TEXT NOT NULL,
              created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
              revoked INTEGER NOT NULL DEFAULT 0
          );
          ALTER TABLE activity ADD COLUMN paired INTEGER NOT NULL DEFAULT 0;",
    },
];

/// Brings the host's tables up to date. `crastu agents` uses it on the same database file.
pub fn migrate(store: &mut Store) -> Result<(), HostError> {
    store.migrate("host", MIGRATIONS)?;
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum HostError {
    #[error(transparent)]
    Store(#[from] crastu_store::StoreError),
    #[error("module registration failed: {0}")]
    Registry(String),
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum CallError {
    #[error("unknown tool: {0}")]
    Unknown(String),
    #[error("{tool} needs approval in the Crastu app ({risk} actions are set to Ask)")]
    NotApproved { tool: String, risk: &'static str },
    #[error("{tool} is denied by the approval policy ({risk} actions are set to Deny)")]
    Denied { tool: String, risk: &'static str },
    #[error("{0}")]
    Tool(String),
    #[error("{0}")]
    Pairing(String),
}

/// Who is calling. Only a presented, valid token makes a caller [`Paired`](Caller::Paired);
/// whatever name an unpaired caller declares is a label for the log and nothing else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Caller<'a> {
    /// No token. Always gets the defaults.
    Unpaired { declared: &'a str },
    /// A token the host issued, resolved to the agent's name.
    Paired { name: &'a str },
}

impl Caller<'_> {
    fn name(&self) -> &str {
        match self {
            Caller::Unpaired { declared } => declared,
            Caller::Paired { name } => name,
        }
    }

    fn is_paired(&self) -> bool {
        matches!(self, Caller::Paired { .. })
    }
}

/// One row of the activity log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Activity {
    /// The paired agent's name, or for an unpaired call the name the client declared.
    pub agent: String,
    /// Whether a valid token backed the call.
    pub paired: bool,
    pub tool: String,
    pub risk: String,
    pub decision: String,
    pub outcome: String,
}

pub(crate) fn risk_name(r: Risk) -> &'static str {
    match r {
        Risk::Read => "read",
        Risk::Change => "change",
        Risk::Delete => "delete",
    }
}

/// State shared with tool handlers.
#[derive(Debug, Clone)]
pub struct Shared {
    pub store: Arc<Mutex<Store>>,
    pub jobs: Arc<JobQueue>,
}

impl Shared {
    pub fn store(&self) -> MutexGuard<'_, Store> {
        self.store.lock().unwrap_or_else(|p| p.into_inner())
    }
}

pub struct Host {
    shared: Shared,
    registry: Registry,
    policy: Mutex<Policy>,
    approver: Box<dyn Approver>,
}

impl std::fmt::Debug for Host {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Host")
            .field("registry", &self.registry)
            .finish_non_exhaustive()
    }
}

impl Host {
    /// Opens the host on `store` with the core module plus `modules`.
    pub fn new(
        mut store: Store,
        jobs: JobQueue,
        modules: Vec<Box<dyn Module>>,
        approver: Box<dyn Approver>,
    ) -> Result<Host, HostError> {
        migrate(&mut store)?;
        let shared = Shared {
            store: Arc::new(Mutex::new(store)),
            jobs: Arc::new(jobs),
        };
        let core = CoreModule::new(shared.clone());
        let mut all: Vec<&dyn Module> = vec![&core];
        all.extend(modules.iter().map(|m| m.as_ref()));
        let registry = Registry::build(&all).map_err(|errs| {
            HostError::Registry(
                errs.iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("; "),
            )
        })?;
        Ok(Host {
            shared,
            registry,
            policy: Mutex::new(Policy::default()),
            approver,
        })
    }

    pub fn shared(&self) -> &Shared {
        &self.shared
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    pub fn set_policy(&self, policy: Policy) {
        *self.policy.lock().unwrap_or_else(|p| p.into_inner()) = policy;
    }

    pub fn tools(&self) -> Vec<ToolInfo> {
        self.registry
            .tools()
            .map(|t| ToolInfo {
                name: t.name.clone(),
                description: t.description.clone(),
                risk: risk_name(t.risk).to_string(),
                input_schema: t.input_schema.clone(),
            })
            .collect()
    }

    /// Runs a tool for a caller after the approval policy, and logs the outcome.
    ///
    /// Per-agent overrides apply only to [`Caller::Paired`]. An unpaired caller gets the defaults
    /// (Change and Delete set to Ask) whatever name it declares.
    pub fn call(&self, caller: Caller<'_>, tool: &str, args: Value) -> Result<Value, CallError> {
        let Some(spec) = self.registry.tools().find(|t| t.name == tool) else {
            return Err(CallError::Unknown(tool.to_string()));
        };
        let risk = spec.risk;
        let decision = {
            let policy = self.policy.lock().unwrap_or_else(|p| p.into_inner());
            match caller {
                Caller::Paired { name } => policy.decide(name, risk),
                Caller::Unpaired { .. } => policy.decide_unpaired(risk),
            }
        };
        let (allowed, decision_name) = match decision {
            Decision::Auto => (true, "auto"),
            Decision::Deny => (false, "deny"),
            Decision::Ask => {
                let ok = self.approver.approve(&ApprovalRequest {
                    agent: caller.name(),
                    paired: caller.is_paired(),
                    tool,
                    risk,
                    args: &args,
                });
                (ok, if ok { "approved" } else { "not approved" })
            }
        };
        if !allowed {
            self.log(caller, tool, risk_name(risk), decision_name, "blocked");
            let risk = risk_name(risk);
            return Err(match decision {
                Decision::Deny => CallError::Denied {
                    tool: tool.into(),
                    risk,
                },
                _ => CallError::NotApproved {
                    tool: tool.into(),
                    risk,
                },
            });
        }
        let result = self.registry.call_tool(tool, args).map_err(|e| match e {
            ToolError::Unknown(n) => CallError::Unknown(n),
            other => CallError::Tool(other.to_string()),
        });
        let outcome = if result.is_ok() { "ok" } else { "error" };
        self.log(caller, tool, risk_name(risk), decision_name, outcome);
        result
    }

    /// Resolves the token of a call (if any) and runs the tool. A token that is presented but
    /// unknown, rotated or revoked is an error and is logged; it never falls back to unpaired.
    fn call_with_token(&self, p: CallParams) -> Result<Value, CallError> {
        let Some(token) = p.token.as_deref() else {
            let caller = Caller::Unpaired { declared: &p.agent };
            return self.call(caller, &p.name, p.args);
        };
        let resolved = pairing::resolve(self.shared.store().conn(), token);
        match resolved {
            Ok(name) => self.call(Caller::Paired { name: &name }, &p.name, p.args),
            Err(e) => {
                let caller = Caller::Unpaired { declared: &p.agent };
                self.log(caller, &p.name, "-", "bad token", "blocked");
                Err(CallError::Pairing(e.to_string()))
            }
        }
    }

    fn log(&self, caller: Caller<'_>, tool: &str, risk: &str, decision: &str, outcome: &str) {
        let store = self.shared.store();
        let _ = store.conn().execute(
            "INSERT INTO activity (agent, tool, risk, decision, outcome, paired)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                caller.name(),
                tool,
                risk,
                decision,
                outcome,
                caller.is_paired()
            ],
        );
    }

    /// The most recent activity entries, newest first.
    pub fn activity(&self, limit: usize) -> Vec<Activity> {
        let store = self.shared.store();
        let Ok(mut stmt) = store.conn().prepare(
            "SELECT agent, tool, risk, decision, outcome, paired FROM activity ORDER BY id DESC LIMIT ?1",
        ) else {
            return Vec::new();
        };
        stmt.query_map([limit as i64], |r| {
            Ok(Activity {
                agent: r.get(0)?,
                tool: r.get(1)?,
                risk: r.get(2)?,
                decision: r.get(3)?,
                outcome: r.get(4)?,
                paired: r.get::<_, i64>(5)? != 0,
            })
        })
        .map(|rows| rows.filter_map(Result::ok).collect())
        .unwrap_or_default()
    }

    /// Answers one IPC request.
    pub fn handle_ipc(&self, method_name: &str, params: Value) -> Result<Value, String> {
        match method_name {
            method::PING => Ok(json!({ "version": env!("CARGO_PKG_VERSION") })),
            method::TOOLS_LIST => serde_json::to_value(self.tools()).map_err(|e| e.to_string()),
            method::TOOLS_CALL => {
                let p: CallParams =
                    serde_json::from_value(params).map_err(|e| format!("bad params: {e}"))?;
                self.call_with_token(p).map_err(|e| e.to_string())
            }
            other => Err(format!("unknown method {other}")),
        }
    }

    /// Binds `path` (errors are returned here, for example when another host already runs) and
    /// serves IPC on a background thread. Join the handle to serve until the process ends.
    #[cfg(unix)]
    pub fn spawn_server(
        self: Arc<Self>,
        path: &std::path::Path,
    ) -> Result<std::thread::JoinHandle<()>, crastu_ipc::IpcError> {
        let listener = crastu_ipc::bind(path)?;
        let host = self.clone();
        Ok(std::thread::spawn(move || {
            crastu_ipc::serve(listener, Arc::new(move |m, p| host.handle_ipc(m, p)))
        }))
    }
}
