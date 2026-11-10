//! Domain services. Pure Rust logic with no knowledge of IPC — commands
//! call into these, which keeps them unit-testable.

pub mod github;
pub mod github_client;
pub mod jira;
pub mod jira_client;
pub mod jira_issues;
pub mod obsidian;
pub mod pull_requests;
pub mod secrets;
pub mod settings;
pub mod todos;
pub mod workstreams;
