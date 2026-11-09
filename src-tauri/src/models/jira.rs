use serde::{Deserialize, Serialize};

/// Non-secret Jira connection details, persisted to `jira.json`.
/// The API token is never stored here — it lives in the OS keychain.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct JiraConfig {
    pub base_url: String,
    pub email: String,
    pub project_keys: Vec<String>,
}

/// What the frontend sees. Mirrored by `JiraSettings` in `src/types/jira.ts`.
/// Only reports whether a token exists; the token itself never crosses IPC outbound.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraSettings {
    pub base_url: String,
    pub email: String,
    pub project_keys: Vec<String>,
    pub has_token: bool,
}

/// Mirrored by `JiraSettingsInput` in `src/types/jira.ts`.
/// `api_token: None` keeps the currently stored token.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraSettingsInput {
    pub base_url: String,
    pub email: String,
    #[serde(default)]
    pub project_keys: Vec<String>,
    pub api_token: Option<String>,
}

/// The authenticated user, as returned by Jira's `/myself` endpoint.
/// Mirrored by `JiraUser` in `src/types/jira.ts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraUser {
    pub display_name: String,
    #[serde(default)]
    pub email_address: Option<String>,
}

/// A Jira issue assigned to the current user, as stored locally.
/// Mirrored by `JiraIssue` in `src/types/jira.ts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraIssue {
    /// Jira's immutable issue id; `key` changes when an issue moves project.
    pub id: String,
    pub key: String,
    pub summary: String,
    /// Name of the work type, e.g. `Task`, `Bug` or `Epic`; empty if unknown.
    pub issue_type: String,
    /// ISO-8601 timestamp of the last change, as reported by Jira.
    pub updated: String,
    pub project_key: String,
    pub status_id: String,
    pub status_name: String,
    /// False for issues kept only because they belong to a workstream.
    pub assigned_to_me: bool,
    pub workstream_id: Option<i64>,
}

/// Mirrored by `JiraIssueList` in `src/types/jira.ts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraIssueList {
    pub issues: Vec<JiraIssue>,
    /// Unix epoch milliseconds of the last successful sync.
    pub last_synced_at: Option<i64>,
}

/// Mirrored by `JiraSyncMode` in `src/types/jira.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum JiraSyncMode {
    /// Replaced all local issues.
    Full,
    /// Applied only issues changed since the last sync.
    Delta,
}

/// Mirrored by `JiraSyncSummary` in `src/types/jira.ts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraSyncSummary {
    pub mode: JiraSyncMode,
    /// Issues inserted or updated.
    pub upserted: usize,
    /// Issues removed because they are no longer assigned to the user (or done).
    pub removed: usize,
    /// Why the related pull requests (searched after the issues were saved) could not
    /// be updated; `None` when they were, or when no GitHub token is set up.
    pub pull_request_error: Option<String>,
}

/// A status available in a project. Mirrored by `JiraProjectStatus` in `src/types/jira.ts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraProjectStatus {
    pub project_key: String,
    pub status_id: String,
    pub name: String,
    /// Jira status category key: `new`, `indeterminate` or `done`.
    pub category: String,
}

/// Identifies one status of one project. Mirrored by `JiraStatusRef` in `src/types/jira.ts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraStatusRef {
    pub project_key: String,
    pub status_id: String,
}

/// Mirrored by `JiraStatusFilter` in `src/types/jira.ts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraStatusFilter {
    /// All statuses of the synced projects, grouped by project and ordered by category.
    pub options: Vec<JiraProjectStatus>,
    /// Statuses to display. Empty means no filter.
    pub selected: Vec<JiraStatusRef>,
}
