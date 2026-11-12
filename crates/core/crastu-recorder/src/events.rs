//! The shapes of `events.jsonl` lines. Job and MCP events are structured on purpose: a name and
//! a state or decision, with no field that takes free text.

use serde_json::{Map, Value, json};

use crastu_redact::ident;

/// Severity of a log line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Info,
    Warn,
    Error,
}

impl Level {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Level::Info => "info",
            Level::Warn => "warn",
            Level::Error => "error",
        }
    }
}

/// Where a job is in its life.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobState {
    Queued,
    Started,
    Finished,
    Failed,
    Cancelled,
}

impl JobState {
    fn as_str(self) -> &'static str {
        match self {
            JobState::Queued => "queued",
            JobState::Started => "started",
            JobState::Finished => "finished",
            JobState::Failed => "failed",
            JobState::Cancelled => "cancelled",
        }
    }
}

/// A job changed state. `kind` is a job kind such as `transcribe`; a value that is not a plain
/// name (letters, digits and `._:-/`, at most 64 bytes) is recorded as `invalid-name`.
#[derive(Clone, Copy, Debug)]
pub struct JobEvent<'a> {
    pub kind: &'a str,
    pub state: JobState,
    pub duration_ms: Option<u64>,
}

/// What the approval policy decided for an MCP call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum McpDecision {
    Allowed,
    Asked,
    Approved,
    Denied,
    Refused,
}

impl McpDecision {
    fn as_str(self) -> &'static str {
        match self {
            McpDecision::Allowed => "allowed",
            McpDecision::Asked => "asked",
            McpDecision::Approved => "approved",
            McpDecision::Denied => "denied",
            McpDecision::Refused => "refused",
        }
    }
}

/// An MCP tool call and what was decided. Only the tool name and the decision: never arguments,
/// results or content.
#[derive(Clone, Copy, Debug)]
pub struct McpEvent<'a> {
    pub tool: &'a str,
    pub decision: McpDecision,
}

pub(crate) fn log_fields(level: Level, target: &str, msg: &str) -> Map<String, Value> {
    let Value::Object(m) = json!({
        "kind": "log",
        "level": level.as_str(),
        "target": crastu_redact::clean(target, 80),
        "msg": crastu_redact::clean(msg, 500),
    }) else {
        return Map::new();
    };
    m
}

pub(crate) fn job_fields(e: &JobEvent<'_>) -> Map<String, Value> {
    let Value::Object(mut m) = json!({
        "kind": "job",
        "job": ident(e.kind),
        "state": e.state.as_str(),
    }) else {
        return Map::new();
    };
    if let Some(ms) = e.duration_ms {
        m.insert("ms".into(), json!(ms));
    }
    m
}

pub(crate) fn mcp_fields(e: &McpEvent<'_>) -> Map<String, Value> {
    let Value::Object(m) = json!({
        "kind": "mcp",
        "tool": ident(e.tool),
        "decision": e.decision.as_str(),
    }) else {
        return Map::new();
    };
    m
}
