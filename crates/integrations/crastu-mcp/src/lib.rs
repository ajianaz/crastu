//! MCP server: exposes Crastu's tools to AI agents over stdio.
//!
//! This crate is a thin bridge. It lists and calls tools through a [`Backend`]: the running host
//! over local IPC (the `crastu-cli mcp` shim) or a host in the same process. The approval policy,
//! the activity log and the tools themselves live in the host, so every front end applies the
//! same rules. stdout carries the protocol; diagnostics go to stderr only.

use std::sync::{Arc, Mutex};

use crastu_ipc::{CallParams, ToolInfo};
use rmcp::ErrorData as McpError;
use rmcp::handler::server::ServerHandler;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ListToolsResult,
    PaginatedRequestParams, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{RoleServer, ServiceExt};
use serde_json::Value;

/// Where tool calls go.
pub trait Backend: Send + Sync + 'static {
    fn list(&self) -> Result<Vec<ToolInfo>, String>;
    fn call(&self, params: CallParams) -> Result<Value, String>;
}

/// A backend that talks to the host over IPC. Calls are serialised over one connection.
#[cfg(unix)]
#[derive(Debug)]
pub struct IpcBackend(Mutex<crastu_ipc::Client>);

#[cfg(unix)]
impl IpcBackend {
    pub fn new(client: crastu_ipc::Client) -> IpcBackend {
        IpcBackend(Mutex::new(client))
    }

    fn with<T>(
        &self,
        f: impl FnOnce(&mut crastu_ipc::Client) -> Result<T, crastu_ipc::IpcError>,
    ) -> Result<T, String> {
        let mut c = self.0.lock().unwrap_or_else(|p| p.into_inner());
        f(&mut c).map_err(|e| e.to_string())
    }
}

#[cfg(unix)]
impl Backend for IpcBackend {
    fn list(&self) -> Result<Vec<ToolInfo>, String> {
        let v = self.with(|c| c.call(crastu_ipc::method::TOOLS_LIST, Value::Null))?;
        serde_json::from_value(v).map_err(|e| e.to_string())
    }

    fn call(&self, params: CallParams) -> Result<Value, String> {
        let p = serde_json::to_value(params).map_err(|e| e.to_string())?;
        self.with(|c| c.call(crastu_ipc::method::TOOLS_CALL, p))
    }
}

/// The rmcp server handler.
#[derive(Clone)]
pub struct McpBridge {
    backend: Arc<dyn Backend>,
    agent: String,
}

impl std::fmt::Debug for McpBridge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpBridge")
            .field("agent", &self.agent)
            .finish_non_exhaustive()
    }
}

impl McpBridge {
    /// `agent` names the client in the approval policy and the activity log.
    pub fn new(backend: Arc<dyn Backend>, agent: impl Into<String>) -> McpBridge {
        McpBridge {
            backend,
            agent: agent.into(),
        }
    }
}

fn to_tool(info: ToolInfo) -> Tool {
    let schema = info.input_schema.as_object().cloned().unwrap_or_default();
    let description = format!("{} (risk: {})", info.description, info.risk);
    Tool::new(info.name, description, Arc::new(schema))
}

impl ServerHandler for McpBridge {
    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let backend = self.backend.clone();
        let tools = tokio::task::spawn_blocking(move || backend.list())
            .await
            .map_err(|e| McpError::internal_error(format!("list task failed: {e}"), None))?
            .map_err(|e| McpError::internal_error(format!("Crastu host: {e}"), None))?;
        Ok(ListToolsResult {
            tools: tools.into_iter().map(to_tool).collect(),
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let params = CallParams {
            agent: self.agent.clone(),
            name: request.name.to_string(),
            args: Value::Object(request.arguments.unwrap_or_default()),
        };
        // Tools can block (jobs, disk); keep them off the async workers.
        let backend = self.backend.clone();
        let result = tokio::task::spawn_blocking(move || backend.call(params))
            .await
            .map_err(|e| McpError::internal_error(format!("tool task failed: {e}"), None))?;
        Ok(match result {
            Ok(v) => CallToolResult::success(vec![ContentBlock::text(
                serde_json::to_string_pretty(&v).unwrap_or_else(|_| v.to_string()),
            )])
            .into(),
            // Policy refusals and tool failures are tool errors the agent can read, not protocol errors.
            Err(e) => CallToolResult::error(vec![ContentBlock::text(e)]).into(),
        })
    }
}

/// Serves MCP on stdin/stdout until the client disconnects.
pub async fn serve_stdio(bridge: McpBridge) -> Result<(), String> {
    let service = bridge
        .serve(rmcp::transport::stdio())
        .await
        .map_err(|e| format!("mcp: initialize failed: {e}"))?;
    service.waiting().await.map_err(|e| format!("mcp: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::RoleClient;
    use rmcp::service::{RunningService, serve_client};
    use serde_json::json;

    /// Mimics the host: one read tool, and a change tool the policy refuses.
    #[derive(Default)]
    struct FakeHost {
        calls: Mutex<Vec<CallParams>>,
    }

    impl Backend for FakeHost {
        fn list(&self) -> Result<Vec<ToolInfo>, String> {
            Ok(vec![
                ToolInfo {
                    name: "project_list".into(),
                    description: "List projects.".into(),
                    risk: "read".into(),
                    input_schema: json!({"type": "object", "properties": {}}),
                },
                ToolInfo {
                    name: "clip_reject".into(),
                    description: "Reject a clip.".into(),
                    risk: "change".into(),
                    input_schema: json!({"type": "object", "properties": {"clip": {"type": "string"}}}),
                },
            ])
        }

        fn call(&self, params: CallParams) -> Result<Value, String> {
            self.calls.lock().unwrap().push(params.clone());
            match params.name.as_str() {
                "project_list" => Ok(json!([{"name": "Money Talk #12"}])),
                "clip_reject" => Err("clip_reject needs approval in the Crastu app".into()),
                other => Err(format!("unknown tool: {other}")),
            }
        }
    }

    async fn connect(host: Arc<FakeHost>) -> RunningService<RoleClient, ()> {
        let (client_out, server_in) = tokio::io::duplex(16 * 1024);
        let (server_out, client_in) = tokio::io::duplex(16 * 1024);
        let bridge = McpBridge::new(host, "claude-code");
        tokio::spawn(async move {
            if let Ok(running) = bridge.serve((server_in, server_out)).await {
                let _ = running.waiting().await;
            }
        });
        serve_client((), (client_in, client_out))
            .await
            .expect("client initialize")
    }

    fn text(res: &CallToolResult) -> String {
        match &res.content[0] {
            ContentBlock::Text(t) => t.text.clone(),
            other => panic!("expected text, got {other:?}"),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn lists_tools_with_their_risk() {
        let client = connect(Arc::new(FakeHost::default())).await;
        let listed = client.peer().list_tools(None).await.unwrap();
        let names: Vec<String> = listed.tools.iter().map(|t| t.name.to_string()).collect();
        assert_eq!(names, vec!["project_list", "clip_reject"]);
        assert!(
            listed.tools[1]
                .description
                .as_deref()
                .unwrap_or_default()
                .contains("risk: change")
        );
        client.cancel().await.ok();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn calls_tools_as_the_named_agent() {
        let host = Arc::new(FakeHost::default());
        let client = connect(host.clone()).await;
        let res = client
            .peer()
            .call_tool(CallToolRequestParams::new("project_list"))
            .await
            .unwrap();
        assert_ne!(res.is_error, Some(true));
        assert!(text(&res).contains("Money Talk #12"));
        assert_eq!(host.calls.lock().unwrap()[0].agent, "claude-code");
        client.cancel().await.ok();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn policy_refusals_are_tool_errors() {
        let client = connect(Arc::new(FakeHost::default())).await;
        let res = client
            .peer()
            .call_tool(
                CallToolRequestParams::new("clip_reject")
                    .with_arguments(json!({"clip": "c1"}).as_object().unwrap().clone()),
            )
            .await
            .unwrap();
        assert_eq!(res.is_error, Some(true));
        assert!(text(&res).contains("needs approval"));
        client.cancel().await.ok();
    }
}
