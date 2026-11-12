//! Settings, Agents over the host's own stores: the same `crastu.db` that `crastu agents` and the
//! host use, so a pairing or a policy change made here acts on the host's next call.
//!
//! The plain token comes back once, from `pair` and `rotate`, and goes nowhere but to the shell;
//! nothing here logs it.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use crastu_host::activity::{self, ActivityEntry};
use crastu_host::pairing;
use crastu_host::policy_store;
use crastu_host::{Decision, Policy};
use crastu_module::Risk;
use crastu_shell::agents::{
    AgentRow, AgentsSnapshot, CallRow, Class, Defaults, Level, RECENT_CALLS, Secret,
};
use crastu_store::{Connection, Store};

fn level(d: Decision) -> Level {
    match d {
        Decision::Auto => Level::Auto,
        Decision::Ask => Level::Ask,
        Decision::Deny => Level::Deny,
    }
}

fn decision(l: Level) -> Decision {
    match l {
        Level::Auto => Decision::Auto,
        Level::Ask => Decision::Ask,
        Level::Deny => Decision::Deny,
    }
}

fn risk(c: Class) -> Risk {
    match c {
        Class::Change => Risk::Change,
        Class::Delete => Risk::Delete,
    }
}

fn call_row(e: ActivityEntry) -> CallRow {
    CallRow {
        at: e.at,
        agent: e.activity.agent,
        paired: e.activity.paired,
        tool: e.activity.tool,
        decision: e.activity.decision,
        outcome: e.activity.outcome,
    }
}

thread_local! {
    /// The connection the UI thread keeps: the page calls the store on every click, and opening
    /// and migrating each time would take the write lock for nothing.
    static OPEN: RefCell<Option<(PathBuf, Store)>> = const { RefCell::new(None) };
}

/// The host's database in the data folder, shared with `crastu agents` and the host. The UI
/// thread opens it once and migrates only when something is pending.
#[derive(Clone, Debug)]
pub struct AgentStore {
    dir: PathBuf,
}

impl AgentStore {
    pub fn new(dir: impl Into<PathBuf>) -> AgentStore {
        AgentStore { dir: dir.into() }
    }

    /// The per-user data folder (`CRASTU_DATA` wins).
    pub fn per_user() -> Result<AgentStore, String> {
        crastu_settings::data_dir().map(AgentStore::new)
    }

    fn open(&self) -> Result<Store, String> {
        prepare(&self.dir)?;
        let mut store = Store::open(self.dir.join("crastu.db")).map_err(|e| e.to_string())?;
        crastu_host::migrate(&mut store).map_err(|e| e.to_string())?;
        Ok(store)
    }

    fn with<T>(&self, f: impl FnOnce(&Connection) -> Result<T, String>) -> Result<T, String> {
        OPEN.with(|cell| {
            let mut slot = cell.borrow_mut();
            if slot.as_ref().is_none_or(|(dir, _)| dir != &self.dir) {
                *slot = Some((self.dir.clone(), self.open()?));
            }
            let (_, store) = slot.as_ref().ok_or("the store is not open")?;
            f(store.conn())
        })
    }

    pub fn snapshot(&self) -> Result<AgentsSnapshot, String> {
        self.with(|conn| {
            let policy = policy_store::load(conn).map_err(|e| e.to_string())?;
            let mut agents = Vec::new();
            for a in pairing::list(conn).map_err(|e| e.to_string())? {
                let last = activity::last_paired_call(conn, &a.name).map_err(|e| e.to_string())?;
                agents.push(AgentRow {
                    change: policy.agent_override(&a.name, Risk::Change).map(level),
                    delete: policy.agent_override(&a.name, Risk::Delete).map(level),
                    last_call: last.map(call_row),
                    name: a.name,
                    created_at: a.created_at,
                    revoked: a.revoked,
                });
            }
            let recent = activity::recent(conn, RECENT_CALLS).map_err(|e| e.to_string())?;
            Ok(AgentsSnapshot {
                agents,
                defaults: defaults(&policy),
                recent: recent.into_iter().map(call_row).collect(),
            })
        })
    }

    pub fn pair(&self, name: &str) -> Result<Secret, String> {
        self.with(|c| {
            pairing::add(c, name)
                .map(Secret::new)
                .map_err(|e| e.to_string())
        })
    }

    pub fn rotate(&self, name: &str) -> Result<Secret, String> {
        self.with(|c| {
            pairing::rotate(c, name)
                .map(Secret::new)
                .map_err(|e| e.to_string())
        })
    }

    pub fn revoke(&self, name: &str) -> Result<(), String> {
        self.with(|c| pairing::revoke(c, name).map_err(|e| e.to_string()))
    }

    pub fn set_default(&self, class: Class, l: Level) -> Result<(), String> {
        self.with(|c| {
            policy_store::set_default(c, risk(class), decision(l)).map_err(|e| e.to_string())
        })
    }

    pub fn set_override(&self, agent: &str, class: Class, l: Option<Level>) -> Result<(), String> {
        self.with(|c| {
            policy_store::set_agent(c, agent, risk(class), l.map(decision))
                .map_err(|e| e.to_string())
        })
    }
}

fn defaults(policy: &Policy) -> Defaults {
    Defaults {
        change: level(policy.change),
        delete: level(policy.delete),
    }
}

/// A private folder and a private database file, without touching the process umask (which
/// would change the mode of every file the app creates afterwards). SQLite gives the -wal and
/// -shm files the mode of the main file, so creating it as 0600 first covers them.
#[cfg(unix)]
fn prepare(dir: &Path) -> Result<(), String> {
    use std::os::unix::fs::OpenOptionsExt;
    let fail = |e: &dyn std::fmt::Display| format!("cannot use {}: {e}", dir.display());
    crastu_ipc::ensure_owned_dir(dir).map_err(|e| fail(&e))?;
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(dir.join("crastu.db"))
        .map(drop)
        .map_err(|e| fail(&e))
}

#[cfg(not(unix))]
fn prepare(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot use {}: {e}", dir.display()))
}

#[cfg(test)]
mod tests;
