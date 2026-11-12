//! Tools every Crastu build has, regardless of which feature modules are built in.

use crastu_core::JobId;
use crastu_jobs::JobStatus;
use crastu_module::{Module, ModuleManifest, Registry, Risk, ToolError, ToolSpec};
use serde_json::{Value, json};

use crate::Shared;

#[derive(Debug)]
pub struct CoreModule {
    shared: Shared,
}

impl CoreModule {
    pub fn new(shared: Shared) -> CoreModule {
        CoreModule { shared }
    }
}

fn job_id(args: &Value) -> Result<JobId, ToolError> {
    args["job"]
        .as_str()
        .ok_or_else(|| ToolError::InvalidArgs("`job` (string id) is required".into()))?
        .parse()
        .map_err(|_| ToolError::InvalidArgs("`job` is not a valid id".into()))
}

fn status_json(s: &JobStatus) -> Value {
    match s {
        JobStatus::Queued => json!({"state": "queued"}),
        JobStatus::Running { progress, message } => {
            json!({"state": "running", "progress": progress, "message": message})
        }
        JobStatus::Done(v) => json!({"state": "done", "result": v}),
        JobStatus::Failed(e) => json!({"state": "failed", "error": e}),
        JobStatus::Cancelled => json!({"state": "cancelled"}),
    }
}

impl Module for CoreModule {
    fn manifest(&self) -> ModuleManifest {
        ModuleManifest {
            id: "core",
            name: "Core",
            version: env!("CARGO_PKG_VERSION"),
        }
    }

    fn register(&self, r: &mut Registry) {
        let shared = self.shared.clone();
        r.tool(
            ToolSpec {
                name: "project_list".into(),
                description: "List Crastu projects, newest first.".into(),
                risk: Risk::Read,
                input_schema: json!({"type": "object", "properties": {}}),
            },
            move |_args: Value| {
                let store = shared.store();
                let mut stmt = store
                    .conn()
                    .prepare("SELECT id, name, created_at FROM projects ORDER BY created_at DESC, id DESC")
                    .map_err(|e| ToolError::Failed(e.to_string()))?;
                let rows = stmt
                    .query_map([], |r| {
                        Ok(json!({"id": r.get::<_, String>(0)?, "name": r.get::<_, String>(1)?, "created_at": r.get::<_, String>(2)?}))
                    })
                    .map_err(|e| ToolError::Failed(e.to_string()))?;
                Ok(Value::Array(rows.filter_map(Result::ok).collect()))
            },
        );

        let shared = self.shared.clone();
        r.tool(
            ToolSpec {
                name: "job_status".into(),
                description: "Get the state and progress of a background job.".into(),
                risk: Risk::Read,
                input_schema: json!({"type": "object", "properties": {"job": {"type": "string"}}, "required": ["job"]}),
            },
            move |args: Value| {
                let id = job_id(&args)?;
                let s = shared.jobs.status(id).ok_or_else(|| ToolError::Failed(format!("no job {id}")))?;
                Ok(status_json(&s))
            },
        );

        let shared = self.shared.clone();
        r.tool(
            ToolSpec {
                name: "job_cancel".into(),
                description: "Cancel a queued or running background job.".into(),
                risk: Risk::Change,
                input_schema: json!({"type": "object", "properties": {"job": {"type": "string"}}, "required": ["job"]}),
            },
            move |args: Value| {
                let id = job_id(&args)?;
                Ok(json!({"cancelled": shared.jobs.cancel(id)}))
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use crate::{DenyAsk, Host};
    use crastu_jobs::{Gate, JobQueue};
    use crastu_store::Store;
    use serde_json::json;

    #[test]
    fn project_list_reads_the_store_and_jobs_are_reachable() {
        let h = Host::new(
            Store::open_in_memory().unwrap(),
            JobQueue::new(1, 1),
            vec![],
            Box::new(DenyAsk),
        )
        .unwrap();
        h.shared()
            .store()
            .conn()
            .execute(
                "INSERT INTO projects (id, name) VALUES ('p1', 'Money Talk #12')",
                [],
            )
            .unwrap();
        let list = h
            .call(
                crate::Caller::Unpaired {
                    declared: "claude-code",
                },
                "project_list",
                json!({}),
            )
            .unwrap();
        assert_eq!(list[0]["name"], "Money Talk #12");

        let id = h.shared().jobs.submit("demo", Gate::Cpu, |_| Ok(json!(1)));
        let status = h
            .call(
                crate::Caller::Unpaired {
                    declared: "claude-code",
                },
                "job_status",
                json!({"job": id.to_string()}),
            )
            .unwrap();
        assert!(["queued", "running", "done"].contains(&status["state"].as_str().unwrap()));
        assert!(
            h.call(
                crate::Caller::Unpaired {
                    declared: "claude-code"
                },
                "job_status",
                json!({"job": "nope"})
            )
            .is_err()
        );
        // job_cancel is a Change tool: a headless host has nobody to ask.
        assert!(
            h.call(
                crate::Caller::Unpaired {
                    declared: "claude-code"
                },
                "job_cancel",
                json!({"job": id.to_string()})
            )
            .is_err()
        );
    }
}
