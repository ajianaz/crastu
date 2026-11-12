//! Agent pairing: a random token per agent, stored only as a SHA-256 hash.
//!
//! An agent name that a client declares is a label, not an identity. A paired agent is one whose
//! token the host has issued: the shim sends the token with every call and the host resolves it
//! to the agent's name, so per-agent overrides apply only to whoever holds the token. The plain
//! token is returned once, by [`add`] or [`rotate`], and never stored. Everything here works on a
//! plain connection, so the host and `crastu agents` (a separate process on the same database)
//! share it; the host looks the token up on every call, so revoke and rotate act at once.

use crastu_store::{Connection, OptionalExtension, params, rusqlite};
use sha2::{Digest, Sha256};

/// Every token starts with this, so a leaked one is easy to recognise and to scan for.
pub const TOKEN_PREFIX: &str = "crastu_";

const NAME_MAX: usize = 64;

#[derive(Debug, thiserror::Error)]
pub enum PairingError {
    #[error("agent names use 1 to {NAME_MAX} characters of a-z, 0-9, '-', '_' and '.'")]
    InvalidName,
    #[error("an agent named {0:?} already exists (use rotate for a new token)")]
    Exists(String),
    #[error("no agent named {0:?}")]
    NotFound(String),
    #[error("agent {0:?} is revoked; pair a new agent under another name")]
    RevokedAgent(String),
    #[error("the agent token is not valid (unknown or rotated); pair the agent again")]
    InvalidToken,
    #[error("the agent token was revoked; pair the agent again")]
    RevokedToken,
    #[error("the system random number generator failed: {0}")]
    Random(String),
    #[error(transparent)]
    Store(#[from] rusqlite::Error),
}

/// One paired agent as listed. The token is never part of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentRecord {
    pub name: String,
    pub created_at: String,
    pub revoked: bool,
}

/// 32 random bytes from the operating system, as `crastu_` plus 64 hex digits.
pub fn generate_token() -> Result<String, PairingError> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| PairingError::Random(e.to_string()))?;
    Ok(format!("{TOKEN_PREFIX}{}", hex(&bytes)))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// What the database keeps instead of the token.
pub fn hash_token(token: &str) -> String {
    hex(&Sha256::digest(token.as_bytes()))
}

/// Compares in time that does not depend on where the first difference is. Different lengths
/// are unequal at once; the hashes compared here always have the same length.
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn check_name(name: &str) -> Result<(), PairingError> {
    let ok = !name.is_empty()
        && name.len() <= NAME_MAX
        && name.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_' | b'.')
        });
    if ok {
        Ok(())
    } else {
        Err(PairingError::InvalidName)
    }
}

/// Pairs a new agent and returns its token, to be shown once.
pub fn add(conn: &Connection, name: &str) -> Result<String, PairingError> {
    check_name(name)?;
    let token = generate_token()?;
    let inserted = conn.execute(
        "INSERT OR IGNORE INTO agents (name, token_hash) VALUES (?1, ?2)",
        params![name, hash_token(&token)],
    )?;
    if inserted == 0 {
        return Err(PairingError::Exists(name.to_string()));
    }
    Ok(token)
}

/// All agents, oldest first.
pub fn list(conn: &Connection) -> Result<Vec<AgentRecord>, PairingError> {
    let mut stmt = conn.prepare("SELECT name, created_at, revoked FROM agents ORDER BY rowid")?;
    let rows = stmt.query_map([], |r| {
        Ok(AgentRecord {
            name: r.get(0)?,
            created_at: r.get(1)?,
            revoked: r.get::<_, i64>(2)? != 0,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Stops the agent's token from working. The record stays, so the name is not reused.
pub fn revoke(conn: &Connection, name: &str) -> Result<(), PairingError> {
    let changed = conn.execute("UPDATE agents SET revoked = 1 WHERE name = ?1", [name])?;
    if changed == 0 {
        return Err(PairingError::NotFound(name.to_string()));
    }
    Ok(())
}

/// Replaces the agent's token; the old one stops working. Returns the new token, shown once.
pub fn rotate(conn: &Connection, name: &str) -> Result<String, PairingError> {
    let revoked: Option<i64> = conn
        .query_row("SELECT revoked FROM agents WHERE name = ?1", [name], |r| {
            r.get(0)
        })
        .optional()?;
    match revoked {
        None => return Err(PairingError::NotFound(name.to_string())),
        Some(r) if r != 0 => return Err(PairingError::RevokedAgent(name.to_string())),
        Some(_) => {}
    }
    let token = generate_token()?;
    let changed = conn.execute(
        "UPDATE agents SET token_hash = ?2 WHERE name = ?1 AND revoked = 0",
        params![name, hash_token(&token)],
    )?;
    // Revoked between the check and the update (another process): no token is handed out.
    if changed == 0 {
        return Err(PairingError::RevokedAgent(name.to_string()));
    }
    Ok(token)
}

/// The agent a token belongs to. An unknown or revoked token is an error, never a fallback to
/// "unpaired": a caller that presents a token is refused when it does not hold.
pub fn resolve(conn: &Connection, token: &str) -> Result<String, PairingError> {
    let presented = hash_token(token);
    let mut stmt = conn.prepare("SELECT name, token_hash, revoked FROM agents")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)? != 0,
        ))
    })?;
    // Every row is compared, so the time does not say which agent matched.
    let mut found: Option<(String, bool)> = None;
    for row in rows {
        let (name, stored, revoked) = row?;
        if ct_eq(stored.as_bytes(), presented.as_bytes()) {
            found = Some((name, revoked));
        }
    }
    match found {
        Some((_, true)) => Err(PairingError::RevokedToken),
        Some((name, false)) => Ok(name),
        None => Err(PairingError::InvalidToken),
    }
}
