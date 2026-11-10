//! A module registers an MCP tool that starts a job; the tool call returns the job id and the
//! job finishes on the queue. This is the end-to-end shape every real module follows.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crastu_jobs::{Gate, JobQueue, JobStatus};
use crastu_module::{Module, ModuleManifest, Registry, Risk, ToolSpec};
use serde_json::{Value, json};

struct Transcriber(Arc<JobQueue>);

impl Module for Transcriber {
    fn manifest(&self) -> ModuleManifest {
        ModuleManifest {
            id: "dictation",
            name: "Dictation",
            version: "0.0.0",
        }
    }

    fn register(&self, r: &mut Registry) {
        let queue = self.0.clone();
        r.tool(
            ToolSpec {
                name: "transcribe_file".into(),
                description: "Transcribe an audio file".into(),
                risk: Risk::Change,
                input_schema: json!({"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}),
            },
            move |args: Value| {
                let path = args["path"].as_str().unwrap_or_default().to_string();
                let id = queue.submit("transcribe", Gate::Gpu, move |ctx| {
                    ctx.progress(0.5, "decoding");
                    Ok(json!({"path": path, "text": "halo"}))
                });
                Ok(json!({"job": id.to_string()}))
            },
        );
    }
}

#[test]
fn tool_call_starts_a_job_that_finishes() {
    let queue = Arc::new(JobQueue::new(1, 1));
    let module = Transcriber(queue.clone());
    let registry = Registry::build(&[&module]).expect("registers");

    let reply = registry
        .call_tool("transcribe_file", json!({"path": "memo.wav"}))
        .unwrap();
    let id = reply["job"].as_str().unwrap().parse().unwrap();

    let start = Instant::now();
    let status = loop {
        let s = queue.status(id).unwrap();
        if s.is_finished() {
            break s;
        }
        assert!(start.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(2));
    };
    assert_eq!(
        status,
        JobStatus::Done(json!({"path": "memo.wav", "text": "halo"}))
    );
}
