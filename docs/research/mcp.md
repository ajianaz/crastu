# MCP in Rust

Researched 2026-11-10. Markers: [V] verified against a primary source in that session; [S] secondary source or search summary; [U] unverified (memory, or not reachable from the sandbox).

Related notes: [rust-ecosystem.md](rust-ecosystem.md).

## Prior art

| Project | Licence | Concept taken | Link |
|---|---|---|---|
| rmcp (modelcontextprotocol/rust-sdk) | Apache-2.0, transitioning from MIT; un-relicensed older contributions remain MIT [V] | The Rust MCP SDK: tool macros, stdio and Streamable HTTP transports, resources, prompts, elicitation | https://github.com/modelcontextprotocol/rust-sdk |
| MCP specification (modelcontextprotocol) | No single licence reported by GitHub (checked 2026-11-13) | Streamable HTTP transport and its security rules (Origin check, loopback bind, authentication); the 2026-07-28 changes | https://github.com/modelcontextprotocol/modelcontextprotocol |
| Claude Code MCP client | Documentation only | How a client registers stdio and HTTP servers (`claude mcp add`), `headersHelper` | https://code.claude.com/docs/en/mcp |
| Claude Desktop | Documentation only [U] | Local servers through `claude_desktop_config.json` or Desktop Extensions (.mcpb) | Not linked: not reachable from the research sandbox |
| tole (MIT, see ARCHITECTURE.md section 9) | MIT | External MCP client of Crastu: risk tiers, approver, rmcp registry patterns | See [ARCHITECTURE.md](../ARCHITECTURE.md#9-related-projects) |

## Findings


- `rmcp` latest **3.9.0**, published 2026-11-05, license **Apache-2.0**, MSRV (rust_version) **1.88** [V]. https://crates.io/api/v1/crates/rmcp/3.9.0
- Repo LICENSE: project transitioning MIT -> Apache-2.0; un-relicensed older contributions remain MIT [V]. https://raw.githubusercontent.com/modelcontextprotocol/rust-sdk/main/LICENSE
- Spec support: implements **2026-07-28**, compatible with 2025-11-25 and earlier; `ProtocolVersion::LATEST = V_2026_07_28`, `LATEST_WITH_INITIALIZE = V_2025_11_25` [V]. https://raw.githubusercontent.com/modelcontextprotocol/rust-sdk/main/README.md ; https://github.com/modelcontextprotocol/rust-sdk/blob/main/crates/rmcp/src/model.rs
- Transports: stdio (`transport-io`), child process client, Streamable HTTP server (Tower service, `transport-streamable-http-server`, `-session`), Streamable HTTP client (reqwest, also unix-socket variant), worker/in-process. Legacy 2024-11-05 HTTP+SSE not provided [V]. Feature list from crates.io API [V].
- Server features: `#[tool]`/`#[tool_router]`/`#[tool_handler]` macros, resources (templates, subscriptions), prompts, completions, progress/cancellation, elicitation (form + URL), tasks (SEP-2663), MRTR; sampling/roots/logging deprecated but functional; OAuth features (`auth`, client-credentials JWT, enterprise-managed) [V].
- **Current spec 2026-07-28** (folder present in spec repo alongside `draft`) [V]: removes `initialize` handshake and `Mcp-Session-Id` (stateless), adds `server/discover`, replaces GET stream / resources subscribe with `subscriptions/listen`, moves tasks to extension `io.modelcontextprotocol/tasks`, introduces MRTR replacing server-initiated sampling/elicitation requests, adds `resultType` [V]. https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/docs/specification/2026-07-28/changelog.mdx . Whether it is formally "released" vs RC not confirmed (modelcontextprotocol.io blocked) [U].
- Streamable HTTP security (2025-11-25 spec text): servers MUST validate `Origin` (403 if invalid) against DNS rebinding; SHOULD bind 127.0.0.1 not 0.0.0.0; SHOULD implement authentication [V]. https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/docs/specification/2025-11-25/basic/transports.mdx
- Clients: Claude Code `claude mcp add --transport stdio <name> -- <cmd>` and `claude mcp add --transport http <name> <url> --header "Authorization: Bearer ..."`; also `headersHelper` for dynamic headers; SSE transport deprecated [V]. https://code.claude.com/docs/en/mcp
- Claude Desktop: local servers via `claude_desktop_config.json` (stdio command) or Desktop Extensions (.mcpb); custom "connectors" for remote HTTP are brokered from Anthropic's cloud, so a localhost HTTP endpoint is likely NOT reachable that way [U].

### Architecture options for a desktop app
- **A. stdio shim (`crastu mcp` CLI)**: client spawns the CLI; it connects to the running GUI over local IPC (Unix domain socket / Windows named pipe, per-user perms) and proxies tool calls; can launch the app if not running. Works with every client incl. Claude Desktop config. rmcp: server on `stdio()`; IPC via tokio + your own protocol, or `transport-streamable-http-client-unix-socket` toward an in-app HTTP-over-UDS server [feature name V, pattern is design].
- **B. Streamable HTTP on 127.0.0.1**: app hosts rmcp Tower service (axum) on loopback, random or fixed port, validates Origin, requires bearer token stored in a user-only file shown in Settings; Claude Code connects via `--transport http --header`. Simpler, no shim, but Claude Desktop needs a shim/bridge [U for Desktop].
- Recommended: ship both, sharing one tool router; stdio shim as the default integration.
