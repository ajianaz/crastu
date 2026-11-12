//! `crastu agents add|list|revoke|rotate`: pairs MCP agents with a token.
//!
//! These work on the host's database in the data folder (`CRASTU_DATA`), so they run with or
//! without a host running: the host reads the pairing on every call, so revoke and rotate act at
//! once. The plain token is printed once, by `add` and `rotate`, and is not stored anywhere.

use std::path::Path;

use crastu_host::pairing::{self, AgentRecord};
use crastu_store::Store;

/// The environment variable the shim reads the token from. Never a command-line argument, which
/// other processes of the user can read from the process list.
pub const TOKEN_ENV: &str = "CRASTU_AGENT_TOKEN";

pub fn run(program: &str, data_dir: &Path, args: &[String]) -> Result<(), String> {
    let usage = || format!("usage: {program} agents <add NAME | list | revoke NAME | rotate NAME>");
    let sub = args.get(1).map(String::as_str);
    let name = args.get(2).map(String::as_str);
    if args.len() > 3 {
        return Err(usage());
    }
    let store = open(data_dir)?;
    let conn = store.conn();
    match (sub, name) {
        (Some("add"), Some(name)) => {
            let token = pairing::add(conn, name).map_err(|e| e.to_string())?;
            println!("{}", token_message(program, name, &token, "Paired agent"));
        }
        (Some("rotate"), Some(name)) => {
            let token = pairing::rotate(conn, name).map_err(|e| e.to_string())?;
            println!(
                "{}",
                token_message(program, name, &token, "New token for agent")
            );
            println!("\nThe old token no longer works.");
        }
        (Some("revoke"), Some(name)) => {
            pairing::revoke(conn, name).map_err(|e| e.to_string())?;
            println!("Revoked agent {name}. Its token no longer works.");
        }
        (Some("list"), None) => {
            let agents = pairing::list(conn).map_err(|e| e.to_string())?;
            print!("{}", format_list(&agents));
        }
        _ => return Err(usage()),
    }
    Ok(())
}

fn open(data_dir: &Path) -> Result<Store, String> {
    // The same care as the host: a private folder, files private to the user.
    crastu_ipc::private_file_mode();
    crastu_ipc::ensure_owned_dir(data_dir)
        .map_err(|e| format!("cannot use {}: {e}", data_dir.display()))?;
    let mut store = Store::open(data_dir.join("crastu.db")).map_err(|e| e.to_string())?;
    crastu_host::migrate(&mut store).map_err(|e| e.to_string())?;
    Ok(store)
}

fn token_message(program: &str, name: &str, token: &str, heading: &str) -> String {
    format!(
        "{heading} {name}. The token is shown once and cannot be shown again:\n\n  {token}\n\n\
         Give it to the agent as an environment variable, not as an argument. For Claude Code:\n\n  \
         claude mcp add --transport stdio --env {TOKEN_ENV}={token} crastu -- {program} mcp"
    )
}

fn format_list(agents: &[AgentRecord]) -> String {
    if agents.is_empty() {
        return "No agents are paired. Add one with: crastu agents add NAME\n".into();
    }
    let width = agents
        .iter()
        .map(|a| a.name.len())
        .max()
        .unwrap_or(4)
        .max(4);
    let mut out = format!("{:width$}  {:24}  STATUS\n", "NAME", "CREATED");
    for a in agents {
        let status = if a.revoked { "revoked" } else { "active" };
        out.push_str(&format!(
            "{:width$}  {:24}  {status}\n",
            a.name, a.created_at
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_shows_names_and_state_and_no_token() {
        let agents = vec![
            AgentRecord {
                name: "claude-code".into(),
                created_at: "2026-11-12T10:00:00.000Z".into(),
                revoked: false,
            },
            AgentRecord {
                name: "tole".into(),
                created_at: "2026-11-12T11:00:00.000Z".into(),
                revoked: true,
            },
        ];
        let out = format_list(&agents);
        assert!(out.contains("claude-code") && out.contains("active"));
        assert!(out.contains("tole") && out.contains("revoked"));
        assert!(!out.contains("crastu_"));
    }

    #[test]
    fn the_add_message_carries_the_token_as_an_environment_variable() {
        let m = token_message("crastu", "tole", "crastu_abc", "Paired agent");
        assert!(m.contains("--env CRASTU_AGENT_TOKEN=crastu_abc"), "{m}");
        assert!(m.contains("-- crastu mcp"), "{m}");
    }
}
