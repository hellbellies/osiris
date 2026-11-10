//! Thin IPC handlers: resolve inputs, call a service, return a typed result.
//! Register new commands in `lib.rs` and add them to `src/lib/ipc.ts`.

pub mod github;
pub mod jira;
pub mod obsidian;
pub mod settings;
pub mod todos;
pub mod workstreams;

use std::time::{SystemTime, UNIX_EPOCH};

/// Current time as Unix epoch milliseconds, the timestamp format stored in the database.
fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as i64)
}
