use super::*;
use crastu_module::{ModuleManifest, ToolSpec};
use std::sync::atomic::{AtomicBool, Ordering};

pub(crate) struct Clips;
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

pub(crate) fn paired(name: &str) -> Caller<'_> {
    Caller::Paired { name }
}

pub(crate) fn unpaired(name: &str) -> Caller<'_> {
    Caller::Unpaired { declared: name }
}

pub(crate) fn host(approve: bool) -> Host {
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
        h.call(unpaired("claude-code"), "project_list", json!({}))
            .unwrap(),
        json!([])
    );
    let log = h.activity(10);
    assert_eq!(
        log[0],
        Activity {
            agent: "claude-code".into(),
            paired: false,
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
        host(true)
            .call(unpaired("tole"), "clip_reject", json!({}))
            .unwrap(),
        json!("rejected")
    );
    let h = host(false);
    let err = h
        .call(unpaired("tole"), "clip_reject", json!({}))
        .unwrap_err();
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
    assert!(h.call(paired("tole"), "clip_reject", json!({})).is_ok());
    assert!(matches!(
        h.call(paired("tole"), "asset_delete", json!({})),
        Err(CallError::Denied { .. })
    ));
    assert_eq!(
        h.call(paired("claude-code"), "asset_delete", json!({}))
            .unwrap(),
        json!("deleted")
    );
}

#[test]
fn unknown_tools_and_ipc_methods() {
    let h = host(true);
    assert_eq!(
        h.call(unpaired("x"), "nope", json!({})),
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
