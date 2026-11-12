//! SQLite storage with FTS5 and per-module versioned migrations.
//!
//! Each module owns its tables and its migration list. Versions start at 1, increase by one, and
//! are applied in a transaction. A database written by a newer Crastu is refused instead of
//! being silently downgraded.

use std::path::Path;

pub use rusqlite::{self, Connection, OptionalExtension, Transaction, params};

/// One schema step for a module.
#[derive(Clone, Copy, Debug)]
pub struct Migration {
    pub version: u32,
    pub sql: &'static str,
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(
        "module {module:?}: migration versions must be 1, 2, 3, ... (found {found} at position {position})"
    )]
    BadMigrationList {
        module: String,
        position: usize,
        found: u32,
    },
    #[error(
        "module {module:?} is at schema version {current}, newer than this build knows ({known}); update Crastu"
    )]
    NewerSchema {
        module: String,
        current: u32,
        known: u32,
    },
}

pub type Result<T> = std::result::Result<T, StoreError>;

#[derive(Debug)]
pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Store> {
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Store> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Store> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS _crastu_migrations (
                module TEXT PRIMARY KEY,
                version INTEGER NOT NULL,
                applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            );",
        )?;
        Ok(Store { conn })
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    /// Current schema version of a module, 0 when it has never migrated.
    pub fn version(&self, module: &str) -> Result<u32> {
        let v: Option<u32> = self
            .conn
            .query_row(
                "SELECT version FROM _crastu_migrations WHERE module = ?1",
                [module],
                |r| r.get(0),
            )
            .optional()?;
        Ok(v.unwrap_or(0))
    }

    /// Applies the pending migrations of one module. Returns how many were applied.
    pub fn migrate(&mut self, module: &str, migrations: &[Migration]) -> Result<u32> {
        for (i, m) in migrations.iter().enumerate() {
            if m.version as usize != i + 1 {
                return Err(StoreError::BadMigrationList {
                    module: module.into(),
                    position: i,
                    found: m.version,
                });
            }
        }
        // The version is read inside the write transaction, so two processes that migrate the
        // same file (the host and a CLI command) cannot both see the old version and apply a
        // step twice: the second waits for the lock and then finds nothing pending.
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let current: u32 = tx
            .query_row(
                "SELECT version FROM _crastu_migrations WHERE module = ?1",
                [module],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0);
        let known = migrations.len() as u32;
        if current > known {
            return Err(StoreError::NewerSchema {
                module: module.into(),
                current,
                known,
            });
        }
        let pending = &migrations[current as usize..];
        if pending.is_empty() {
            return Ok(0);
        }
        for m in pending {
            tx.execute_batch(m.sql)?;
        }
        tx.execute(
            "INSERT INTO _crastu_migrations (module, version) VALUES (?1, ?2)
             ON CONFLICT(module) DO UPDATE SET version = excluded.version,
                applied_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
            params![module, known],
        )?;
        tx.commit()?;
        Ok(pending.len() as u32)
    }

    /// Whether this SQLite build has the FTS5 extension (true for the bundled build).
    pub fn fts5_available(&self) -> bool {
        self.conn
            .query_row("SELECT sqlite_compileoption_used('ENABLE_FTS5')", [], |r| {
                r.get::<_, i64>(0)
            })
            .map(|v| v == 1)
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTES: &[Migration] = &[
        Migration {
            version: 1,
            sql: "CREATE TABLE notes (id INTEGER PRIMARY KEY, body TEXT NOT NULL);",
        },
        Migration {
            version: 2,
            sql: "CREATE VIRTUAL TABLE notes_fts USING fts5(body, content='notes', content_rowid='id');",
        },
    ];

    #[test]
    fn migrates_once_and_tracks_versions_per_module() {
        let mut s = Store::open_in_memory().unwrap();
        assert_eq!(s.version("notes").unwrap(), 0);
        assert_eq!(s.migrate("notes", &NOTES[..1]).unwrap(), 1);
        assert_eq!(s.migrate("notes", NOTES).unwrap(), 1);
        assert_eq!(s.migrate("notes", NOTES).unwrap(), 0);
        assert_eq!(s.version("notes").unwrap(), 2);
        assert_eq!(s.version("other").unwrap(), 0);
    }

    #[test]
    fn full_text_search_works() {
        let mut s = Store::open_in_memory().unwrap();
        assert!(s.fts5_available());
        s.migrate("notes", NOTES).unwrap();
        let c = s.conn();
        c.execute("INSERT INTO notes (body) VALUES ('dana darurat minimal enam bulan'), ('emas bukan investasi')", [])
            .unwrap();
        c.execute("INSERT INTO notes_fts (notes_fts) VALUES ('rebuild')", [])
            .unwrap();
        let hit: i64 = c
            .query_row(
                "SELECT rowid FROM notes_fts WHERE notes_fts MATCH 'darurat'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hit, 1);
    }

    #[test]
    fn rejects_bad_lists_and_newer_databases() {
        let mut s = Store::open_in_memory().unwrap();
        let gap = [NOTES[1]];
        assert!(matches!(
            s.migrate("notes", &gap),
            Err(StoreError::BadMigrationList { .. })
        ));
        s.migrate("notes", NOTES).unwrap();
        assert!(matches!(
            s.migrate("notes", &NOTES[..1]),
            Err(StoreError::NewerSchema {
                current: 2,
                known: 1,
                ..
            })
        ));
    }

    #[test]
    fn failed_migration_rolls_back() {
        let mut s = Store::open_in_memory().unwrap();
        let broken = [Migration {
            version: 1,
            sql: "CREATE TABLE t (a INTEGER); THIS IS NOT SQL;",
        }];
        assert!(s.migrate("broken", &broken).is_err());
        assert_eq!(s.version("broken").unwrap(), 0);
        let exists: i64 = s
            .conn()
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name = 't'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(exists, 0);
    }

    #[test]
    fn processes_migrating_the_same_file_apply_each_step_once() {
        // `CREATE TABLE` without IF NOT EXISTS fails if a step runs twice.
        const ONCE: &[Migration] = &[Migration {
            version: 1,
            sql: "CREATE TABLE once (a INTEGER);",
        }];
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crastu.db");
        Store::open(&path).unwrap(); // the file and its journal mode exist before the race
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(6));
        let handles: Vec<_> = (0..6)
            .map(|_| {
                let (path, barrier) = (path.clone(), barrier.clone());
                std::thread::spawn(move || {
                    let mut s = Store::open(&path).unwrap();
                    barrier.wait();
                    s.migrate("once", ONCE)
                })
            })
            .collect();
        let applied: Vec<u32> = handles
            .into_iter()
            .map(|h| h.join().unwrap().expect("no contender may fail"))
            .collect();
        assert_eq!(
            applied.iter().sum::<u32>(),
            1,
            "exactly one applies it: {applied:?}"
        );
    }

    #[test]
    fn persists_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crastu.db");
        Store::open(&path).unwrap().migrate("notes", NOTES).unwrap();
        assert_eq!(Store::open(&path).unwrap().version("notes").unwrap(), 2);
    }
}
