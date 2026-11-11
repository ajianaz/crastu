//! Headless core process: runs modules, jobs, store and engines without a window.
//!
//! Every tool call from an agent goes through [`Host::call`]: the approval policy decides Auto,
//! Ask or Deny for the tool's risk, an [`Approver`] answers Ask, and the outcome is written to the
//! activity log. The GUI, the CLI and the MCP shim all reach the host the same way, in process or
//! over [`crastu_ipc`].

mod core_module;
mod policy;

use std::sync::{Arc, Mutex, MutexGuard};

use crastu_ipc::{CallParams, ToolInfo, method};
use crastu_jobs::JobQueue;
use crastu_module::{Module, Registry, Risk, ToolError};
use crastu_store::{Migration, Store, params};
use serde_json::{Value, json};

pub use core_module::CoreModule;
pub use policy::{ApprovalRequest, Approver, Decision, DenyAsk, Policy};

const MIGRATIONS: &[Migration] = &[Migration {
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
}];

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
}

/// One row of the activity log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Activity {
    pub agent: String,
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
        store.migrate("host", MIGRATIONS)?;
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

    /// Runs a tool for an agent after the approval policy, and logs the outcome.
    ///
    /// `agent` is the name the client declares (`crastu mcp --agent`); it is not authenticated.
    /// Any process of the same user can claim any name, so per-agent overrides are a convenience,
    /// not a security boundary. The defaults (Change and Delete set to Ask) apply to every name.
    pub fn call(&self, agent: &str, tool: &str, args: Value) -> Result<Value, CallError> {
        let Some(spec) = self.registry.tools().find(|t| t.name == tool) else {
            return Err(CallError::Unknown(tool.to_string()));
        };
        let risk = spec.risk;
        let decision = self
            .policy
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .decide(agent, risk);
        let (allowed, decision_name) = match decision {
            Decision::Auto => (true, "auto"),
            Decision::Deny => (false, "deny"),
            Decision::Ask => {
                let ok = self.approver.approve(&ApprovalRequest {
                    agent,
                    tool,
                    risk,
                    args: &args,
                });
                (ok, if ok { "approved" } else { "not approved" })
            }
        };
        if !allowed {
            self.log(agent, tool, risk, decision_name, "blocked");
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
        self.log(
            agent,
            tool,
            risk,
            decision_name,
            if result.is_ok() { "ok" } else { "error" },
        );
        result
    }

    fn log(&self, agent: &str, tool: &str, risk: Risk, decision: &str, outcome: &str) {
        let store = self.shared.store();
        let _ = store.conn().execute(
            "INSERT INTO activity (agent, tool, risk, decision, outcome) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![agent, tool, risk_name(risk), decision, outcome],
        );
    }

    /// The most recent activity entries, newest first.
    pub fn activity(&self, limit: usize) -> Vec<Activity> {
        let store = self.shared.store();
        let Ok(mut stmt) = store.conn().prepare(
            "SELECT agent, tool, risk, decision, outcome FROM activity ORDER BY id DESC LIMIT ?1",
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
                self.call(&p.agent, &p.name, p.args)
                    .map_err(|e| e.to_string())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crastu_module::{ModuleManifest, ToolSpec};
    use std::sync::atomic::{AtomicBool, Ordering};

    struct Clips;
    impl Module for Clips {
        fn manifest(&self) -> ModuleManifest {
            ModuleManifest {
                id: "clipper",
                name: "Clipper",
                version: "0.0.0",
            }
        }
        fn register(&self, r: &mut Registry) {
            r.tool(
                ToolSpec {
                    name: "clip_reject".into(),
                    description: "Reject".into(),
                    risk: Risk::Change,
                    input_schema: json!({}),
                },
                |_a: Value| Ok(json!("rejected")),
            );
            r.tool(
                ToolSpec {
                    name: "asset_delete".into(),
                    description: "Delete".into(),
                    risk: Risk::Delete,
                    input_schema: json!({}),
                },
                |_a: Value| Ok(json!("deleted")),
            );
        }
    }

    struct Answer(AtomicBool);
    impl Approver for Answer {
        fn approve(&self, _r: &ApprovalRequest<'_>) -> bool {
            self.0.load(Ordering::Relaxed)
        }
    }

    fn host(approve: bool) -> Host {
        Host::new(
            Store::open_in_memory().unwrap(),
            JobQueue::new(1, 1),
            vec![Box::new(Clips)],
            Box::new(Answer(AtomicBool::new(approve))),
        )
        .unwrap()
    }

    #[test]
    fn reads_run_automatically_and_are_logged() {
        let h = host(false);
        assert_eq!(
            h.call("claude-code", "project_list", json!({})).unwrap(),
            json!([])
        );
        let log = h.activity(10);
        assert_eq!(
            log[0],
            Activity {
                agent: "claude-code".into(),
                tool: "project_list".into(),
                risk: "read".into(),
                decision: "auto".into(),
                outcome: "ok".into()
            }
        );
    }

    #[test]
    fn ask_uses_the_approver() {
        assert_eq!(
            host(true).call("tole", "clip_reject", json!({})).unwrap(),
            json!("rejected")
        );
        let h = host(false);
        let err = h.call("tole", "clip_reject", json!({})).unwrap_err();
        assert_eq!(
            err,
            CallError::NotApproved {
                tool: "clip_reject".into(),
                risk: "change"
            }
        );
        assert_eq!(h.activity(1)[0].outcome, "blocked");
    }

    #[test]
    fn policy_can_auto_approve_or_deny_per_agent() {
        let h = host(false);
        let mut p = Policy::default();
        p.change = Decision::Auto;
        p.set_agent("tole", Risk::Delete, Decision::Deny);
        p.set_agent("claude-code", Risk::Delete, Decision::Auto);
        h.set_policy(p);
        assert!(h.call("tole", "clip_reject", json!({})).is_ok());
        assert!(matches!(
            h.call("tole", "asset_delete", json!({})),
            Err(CallError::Denied { .. })
        ));
        assert_eq!(
            h.call("claude-code", "asset_delete", json!({})).unwrap(),
            json!("deleted")
        );
    }

    #[test]
    fn unknown_tools_and_ipc_methods() {
        let h = host(true);
        assert_eq!(
            h.call("x", "nope", json!({})),
            Err(CallError::Unknown("nope".into()))
        );
        let tools: Vec<ToolInfo> =
            serde_json::from_value(h.handle_ipc(method::TOOLS_LIST, Value::Null).unwrap()).unwrap();
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "asset_delete",
                "clip_reject",
                "job_cancel",
                "job_status",
                "project_list"
            ]
        );
        assert!(h.handle_ipc("bogus", Value::Null).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn serves_over_ipc() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("host.sock");
        let h = Arc::new(host(true));
        h.clone().spawn_server(&path).unwrap();
        let mut c = crastu_ipc::Client::connect(&path).unwrap();
        assert_eq!(
            c.call(method::PING, Value::Null).unwrap()["version"],
            env!("CARGO_PKG_VERSION")
        );
        let out = c
            .call(
                method::TOOLS_CALL,
                json!({"agent": "claude-code", "name": "clip_reject", "args": {}}),
            )
            .unwrap();
        assert_eq!(out, json!("rejected"));
    }
}
