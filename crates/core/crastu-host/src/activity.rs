//! Reading the activity log without a running host: Settings, Agents shows the last calls of an
//! agent from the same database.

use crastu_store::{Connection, rusqlite};

use crate::Activity;

/// One logged call with its time (UTC, ISO 8601).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivityEntry {
    pub at: String,
    pub activity: Activity,
}

const COLUMNS: &str = "at, agent, tool, risk, decision, outcome, paired";

fn entry(r: &rusqlite::Row<'_>) -> rusqlite::Result<ActivityEntry> {
    Ok(ActivityEntry {
        at: r.get(0)?,
        activity: Activity {
            agent: r.get(1)?,
            tool: r.get(2)?,
            risk: r.get(3)?,
            decision: r.get(4)?,
            outcome: r.get(5)?,
            paired: r.get::<_, i64>(6)? != 0,
        },
    })
}

/// The newest `limit` calls, newest first.
pub fn recent(conn: &Connection, limit: usize) -> Result<Vec<ActivityEntry>, rusqlite::Error> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM activity ORDER BY id DESC LIMIT ?1"
    ))?;
    let rows = stmt.query_map([limit as i64], entry)?;
    rows.collect()
}

/// The newest call a paired agent made, if any. An unpaired caller that declared the same name
/// does not count: it is a label, not the agent.
pub fn last_paired_call(
    conn: &Connection,
    agent: &str,
) -> Result<Option<ActivityEntry>, rusqlite::Error> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM activity WHERE agent = ?1 AND paired = 1 ORDER BY id DESC LIMIT 1"
    ))?;
    let mut rows = stmt.query_map([agent], entry)?;
    rows.next().transpose()
}
