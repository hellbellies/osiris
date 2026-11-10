use serde::{Deserialize, Serialize};

use crate::models::github::PullRequest;
use crate::models::jira::JiraIssue;
use crate::models::obsidian::ObsidianNote;
use crate::models::todos::Todo;

/// Mirrored by `Workstream` in `src/types/workstreams.ts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Workstream {
    pub id: i64,
    pub name: String,
    /// Unix epoch milliseconds.
    pub created_at: i64,
    /// Unix epoch milliseconds when marked completed; `None` while active.
    pub completed_at: Option<i64>,
    /// Number of linked items of all types.
    pub item_count: i64,
}

/// A workstream with its items, grouped by item type.
/// Mirrored by `WorkstreamDetail` in `src/types/workstreams.ts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkstreamDetail {
    pub workstream: Workstream,
    pub jira_issues: Vec<JiraIssue>,
    pub pull_requests: Vec<PullRequest>,
    pub todos: Vec<Todo>,
    pub obsidian_notes: Vec<ObsidianNote>,
}

/// Items of every type matching a search term, to pick from when building a workstream.
/// Mirrored by `WorkstreamItemSearch` in `src/types/workstreams.ts`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkstreamItemSearch {
    pub jira_issues: Vec<JiraIssue>,
    pub pull_requests: Vec<PullRequest>,
    pub todos: Vec<Todo>,
    /// Notes in the open Obsidian vaults; filled in by the command (needs the CLI).
    pub obsidian_notes: Vec<ObsidianNote>,
    /// Why Obsidian couldn't be searched, e.g. because it isn't running.
    pub obsidian_error: Option<String>,
}

/// Kinds of items a workstream can hold. Mirrored by `WorkstreamItemType` in `src/types/workstreams.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkstreamItemType {
    JiraIssue,
    PullRequest,
    Todo,
    ObsidianNote,
}

impl WorkstreamItemType {
    /// Value stored in `workstream_items.item_type`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::JiraIssue => "jira_issue",
            Self::PullRequest => "github_pr",
            Self::Todo => "todo",
            Self::ObsidianNote => "obsidian_note",
        }
    }
}

/// Identifies one item of any type. Mirrored by `WorkstreamItemRef` in `src/types/workstreams.ts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkstreamItemRef {
    pub item_type: WorkstreamItemType,
    /// The item's id in its own table, e.g. the Jira issue id.
    pub item_id: String,
}
