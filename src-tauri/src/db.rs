//! SQLite database: connection setup and schema migrations.
//! Add a migration by appending a new `M::up(include_str!(...))` — never edit an existing one.

use std::fs;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use rusqlite::Connection;
use rusqlite_migration::{M, Migrations};

use crate::error::AppResult;

pub const DB_FILE: &str = "osiris.db";

const MIGRATIONS_SLICE: &[M<'_>] = &[
    M::up(include_str!("../migrations/001_jira_issues.sql")),
    M::up(include_str!("../migrations/002_jira_status_filter.sql")),
    M::up(include_str!("../migrations/003_workstreams.sql")),
    M::up(include_str!("../migrations/004_github_pull_requests.sql")),
    M::up(include_str!("../migrations/005_workstream_completion.sql")),
    M::up(include_str!("../migrations/006_todos.sql")),
    M::up(include_str!("../migrations/007_jira_issue_type.sql")),
    M::up(include_str!("../migrations/008_other_pull_requests.sql")),
    M::up(include_str!(
        "../migrations/009_jira_issue_pull_requests.sql"
    )),
    M::up(include_str!(
        "../migrations/010_jira_issue_pull_request_searches.sql"
    )),
    M::up(include_str!("../migrations/011_obsidian_notes.sql")),
];
const MIGRATIONS: Migrations<'_> = Migrations::from_slice(MIGRATIONS_SLICE);

/// The app's database connection, managed as `tauri::State`.
pub struct Db(Mutex<Connection>);

impl Db {
    pub fn new(conn: Connection) -> Self {
        Self(Mutex::new(conn))
    }

    /// Locks the connection. Never hold the guard across an `.await`.
    pub fn lock(&self) -> MutexGuard<'_, Connection> {
        // A panic mid-transaction rolls the transaction back, so the data is still consistent.
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Opens (or creates) the database in `dir` and migrates it to the latest schema.
pub fn open(dir: &Path) -> AppResult<Connection> {
    fs::create_dir_all(dir)?;
    let mut conn = Connection::open(dir.join(DB_FILE))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    MIGRATIONS.to_latest(&mut conn)?;
    Ok(conn)
}

/// In-memory database with the full schema, for tests.
#[cfg(test)]
pub fn open_in_memory() -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    MIGRATIONS.to_latest(&mut conn).unwrap();
    conn
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_valid() {
        MIGRATIONS.validate().unwrap();
    }

    #[test]
    fn open_creates_file_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        open(dir.path()).unwrap();
        open(dir.path()).unwrap();
        assert!(dir.path().join(DB_FILE).exists());
    }
}
