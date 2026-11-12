//! The approval policy in the database, so the app that edits it (Settings, Agents) and the host
//! that applies it can be different processes, and a change acts on the next call.
//!
//! One table, `agent_policy`. A row with an empty agent is the default for a call class; a row
//! with a name is that agent's override. Names are never empty, so the two cannot collide.

use crastu_module::Risk;
use crastu_store::{Connection, params, rusqlite};

use crate::pairing::{PairingError, check_name};
use crate::policy::{Decision, Policy};
use crate::risk_name;

#[derive(Debug, thiserror::Error)]
pub enum PolicyError {
    #[error("that setting is not allowed for this kind of action")]
    NotAllowed,
    #[error("a stored policy row is not understood or not allowed ({0}); fix or delete it")]
    Damaged(String),
    #[error(transparent)]
    Name(#[from] PairingError),
    #[error(transparent)]
    Store(#[from] rusqlite::Error),
}

fn risk_from(name: &str) -> Option<Risk> {
    match name {
        "read" => Some(Risk::Read),
        "change" => Some(Risk::Change),
        "delete" => Some(Risk::Delete),
        _ => None,
    }
}

fn decision_name(d: Decision) -> &'static str {
    match d {
        Decision::Auto => "auto",
        Decision::Ask => "ask",
        Decision::Deny => "deny",
    }
}

fn decision_from(name: &str) -> Option<Decision> {
    match name {
        "auto" => Some(Decision::Auto),
        "ask" => Some(Decision::Ask),
        "deny" => Some(Decision::Deny),
        _ => None,
    }
}

/// The stored policy. A row that is not understood or not allowed is an error, not something
/// to skip: skipping it could drop a Deny or an Ask and leave a looser policy. The host treats an
/// error as "reads only" and Settings shows it.
pub fn load(conn: &Connection) -> Result<Policy, PolicyError> {
    let mut stmt = conn.prepare("SELECT agent, risk, decision FROM agent_policy ORDER BY rowid")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })?;
    let mut policy = Policy::default();
    for row in rows {
        let (agent, risk_text, decision_text) = row?;
        let damaged = || PolicyError::Damaged(format!("{agent:?} {risk_text} {decision_text}"));
        let (Some(risk), Some(decision)) = (risk_from(&risk_text), decision_from(&decision_text))
        else {
            return Err(damaged());
        };
        let accepted = if agent.is_empty() {
            policy.set_default(risk, decision)
        } else {
            policy.set_agent(&agent, risk, decision);
            policy.agent_override(&agent, risk) == Some(decision)
        };
        if !accepted {
            return Err(damaged());
        }
    }
    Ok(policy)
}

fn put(conn: &Connection, agent: &str, risk: Risk, d: Decision) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO agent_policy (agent, risk, decision) VALUES (?1, ?2, ?3)
         ON CONFLICT (agent, risk) DO UPDATE SET decision = excluded.decision",
        params![agent, risk_name(risk), decision_name(d)],
    )?;
    Ok(())
}

/// Sets the default decision of a call class.
pub fn set_default(conn: &Connection, risk: Risk, decision: Decision) -> Result<(), PolicyError> {
    if !Policy::default().set_default(risk, decision) {
        return Err(PolicyError::NotAllowed);
    }
    Ok(put(conn, "", risk, decision)?)
}

/// Sets (`Some`) or removes (`None`) one agent's override for a call class.
pub fn set_agent(
    conn: &Connection,
    agent: &str,
    risk: Risk,
    decision: Option<Decision>,
) -> Result<(), PolicyError> {
    check_name(agent)?;
    let Some(decision) = decision else {
        conn.execute(
            "DELETE FROM agent_policy WHERE agent = ?1 AND risk = ?2",
            params![agent, risk_name(risk)],
        )?;
        return Ok(());
    };
    let mut probe = Policy::default();
    probe.set_agent(agent, risk, decision);
    if probe.agent_override(agent, risk) != Some(decision) {
        return Err(PolicyError::NotAllowed);
    }
    Ok(put(conn, agent, risk, decision)?)
}

/// Runs `f` in one write transaction. Every failure path, a failed COMMIT included, ends in a
/// ROLLBACK, so the shared connection is never left inside a transaction.
fn in_transaction(
    conn: &Connection,
    f: impl FnOnce() -> Result<(), rusqlite::Error>,
) -> Result<(), rusqlite::Error> {
    conn.execute_batch("BEGIN IMMEDIATE")?;
    let done = f().and_then(|()| conn.execute_batch("COMMIT"));
    if done.is_err() && !conn.is_autocommit() {
        let _ = conn.execute_batch("ROLLBACK");
    }
    done
}

/// Stores `policy` as a whole, replacing what was there, in one transaction: a reader never
/// sees the table half written or empty, and a failure leaves the old policy.
pub fn replace(conn: &Connection, policy: &Policy) -> Result<(), rusqlite::Error> {
    in_transaction(conn, || {
        conn.execute("DELETE FROM agent_policy", [])?;
        for (risk, d) in [(Risk::Change, policy.change), (Risk::Delete, policy.delete)] {
            put(conn, "", risk, d)?;
        }
        for (agent, risk, d) in policy.overrides() {
            put(conn, agent, risk, d)?;
        }
        Ok(())
    })
}

/// Replaces the whole stored policy with the defaults (Ask for changes and deletions, no
/// overrides), but only while it is still damaged, decided inside the same write transaction. Returns false, and changes nothing, when the policy reads fine
/// (fixed from the CLI or another window since the page looked): a healthy policy is
/// never wiped by a stale confirmation.
pub fn reset_if_damaged(conn: &Connection) -> Result<bool, rusqlite::Error> {
    let reset_done = std::cell::Cell::new(false);
    in_transaction(conn, || {
        if matches!(load(conn), Err(PolicyError::Damaged(_))) {
            conn.execute("DELETE FROM agent_policy", [])?;
            for risk in [Risk::Change, Risk::Delete] {
                put(conn, "", risk, Decision::Ask)?;
            }
            reset_done.set(true);
        }
        Ok(())
    })?;
    Ok(reset_done.get())
}
