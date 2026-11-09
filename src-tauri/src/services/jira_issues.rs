//! Local cache of Jira issues and the full/delta sync that fills it.
//!
//! The cache holds the issues assigned to the user that aren't done, plus issues
//! that belong to a workstream (kept with `assigned_to_me = 0` after being
//! reassigned or done). Unlinked issues that are no longer assigned are pruned.
//!
//! A full sync re-fetches all assigned, not done issues and every workstream-linked
//! issue. A delta sync asks Jira only for issues in the configured projects updated
//! since the last sync (relative JQL, so time zones don't matter), split into those
//! assigned to the user and not done, and the rest.
//!
//! Issues deleted in Jira are only noticed by a full sync.
//!
//! Every sync also refreshes the statuses each project can have; the user's
//! status filter refers to those (project, status) pairs.

use std::collections::HashSet;

use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::models::jira::{
    JiraIssue, JiraIssueList, JiraProjectStatus, JiraStatusFilter, JiraStatusRef, JiraSyncMode,
    JiraSyncSummary,
};
use crate::services::jira_client::JiraClient;

/// JQL for the issues the user works on: assigned to them and not done.
const MINE: &str = "assignee = currentUser() AND statusCategory != Done";

/// Extra minutes added to the delta window to absorb clock skew and search-index lag.
const DELTA_MARGIN_MINUTES: i64 = 5;
/// Issue ids per `id in (…)` query, keeping the JQL reasonably short.
const ID_QUERY_CHUNK: usize = 100;

/// Columns read by [`issue_from_row`]; `i` is `jira_issues`, `w` is `workstream_items`.
const ISSUE_COLUMNS: &str = "i.id, i.key, i.summary, i.issue_type, i.updated, i.project_key,
     i.status_id, i.status_name, i.assigned_to_me, w.workstream_id";
/// Joins each issue to its (optional) workstream link.
const ISSUE_FROM: &str = "jira_issues i LEFT JOIN workstream_items w
     ON w.item_type = 'jira_issue' AND w.item_id = i.id";

/// What the next sync has to do.
#[derive(Debug, PartialEq, Eq)]
enum SyncPlan {
    Full,
    /// Fetch issues updated within the last `minutes`.
    Delta {
        minutes: i64,
    },
}

struct SyncState {
    scope: String,
    last_synced_at: i64,
}

/// Issues assigned to the user, most recently updated first.
pub fn list(conn: &Connection) -> AppResult<JiraIssueList> {
    let issues = conn
        .prepare(&format!(
            "SELECT {ISSUE_COLUMNS} FROM {ISSUE_FROM}
             WHERE i.assigned_to_me = 1 ORDER BY i.updated DESC, i.key"
        ))?
        .query_map([], issue_from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(JiraIssueList {
        issues,
        last_synced_at: load_state(conn)?.map(|state| state.last_synced_at),
    })
}

/// Issues linked to a workstream, including ones no longer assigned to the user.
pub fn list_for_workstream(conn: &Connection, workstream_id: i64) -> AppResult<Vec<JiraIssue>> {
    Ok(conn
        .prepare(&format!(
            "SELECT {ISSUE_COLUMNS} FROM {ISSUE_FROM}
             WHERE w.workstream_id = ?1 ORDER BY i.updated DESC, i.key"
        ))?
        .query_map([workstream_id], issue_from_row)?
        .collect::<Result<Vec<_>, _>>()?)
}

/// Cached issues whose key or summary contains `term` (ASCII case-insensitive),
/// most recently updated first.
pub fn search(conn: &Connection, term: &str, limit: i64) -> AppResult<Vec<JiraIssue>> {
    Ok(conn
        .prepare(&format!(
            "SELECT {ISSUE_COLUMNS} FROM {ISSUE_FROM}
             WHERE instr(lower(i.key || ' ' || i.summary), lower(?1)) > 0
             ORDER BY i.updated DESC, i.key LIMIT ?2"
        ))?
        .query_map(params![term, limit], issue_from_row)?
        .collect::<Result<Vec<_>, _>>()?)
}

/// Whether an issue with this id is cached.
pub fn exists(conn: &Connection, id: &str) -> AppResult<bool> {
    Ok(conn
        .query_row("SELECT 1 FROM jira_issues WHERE id = ?1", [id], |_| Ok(()))
        .optional()?
        .is_some())
}

/// Deletes issues that are neither assigned to the user nor linked to a workstream.
/// Call after anything that can unlink issues. Returns the number removed.
pub fn prune_unlinked(conn: &Connection) -> AppResult<usize> {
    Ok(conn.execute(
        "DELETE FROM jira_issues WHERE assigned_to_me = 0 AND id NOT IN
             (SELECT item_id FROM workstream_items WHERE item_type = 'jira_issue')",
        [],
    )?)
}

fn issue_from_row(row: &Row<'_>) -> rusqlite::Result<JiraIssue> {
    Ok(JiraIssue {
        id: row.get(0)?,
        key: row.get(1)?,
        summary: row.get(2)?,
        issue_type: row.get(3)?,
        updated: row.get(4)?,
        project_key: row.get(5)?,
        status_id: row.get(6)?,
        status_name: row.get(7)?,
        assigned_to_me: row.get(8)?,
        workstream_id: row.get(9)?,
    })
}

/// Fetches the user's issues in `project_keys` and stores them.
/// Runs a delta sync when possible; `force_full` or a changed site/account/project
/// selection triggers a full sync. `now_ms` is recorded as the sync time.
pub async fn sync(
    db: &Db,
    client: &JiraClient,
    project_keys: &[String],
    force_full: bool,
    now_ms: i64,
) -> AppResult<JiraSyncSummary> {
    if project_keys.is_empty() {
        return Err(AppError::Validation(
            "Add at least one project key in the Jira settings".into(),
        ));
    }
    let scope = scope(client, project_keys);
    // The lock is released at the end of this statement — never hold it across `.await`.
    let state = load_state(&db.lock())?;
    let projects = jql_projects(project_keys);

    let (mode, mine, others) = match plan(state.as_ref(), &scope, force_full, now_ms) {
        SyncPlan::Full => {
            let jql = format!("{projects} AND {MINE} ORDER BY updated DESC");
            let mine = client.search(&jql).await?;
            // Refresh linked issues that are no longer (or never were) assigned to the user,
            // wherever they live. Ids come from Jira, but keep only numeric ones for safe JQL.
            let mine_ids: HashSet<&str> = mine.iter().map(|issue| issue.id.as_str()).collect();
            let linked_ids: Vec<String> = linked_issue_ids(&db.lock())?
                .into_iter()
                .filter(|id| !mine_ids.contains(id.as_str()))
                .filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
                .collect();
            let mut others = Vec::new();
            for chunk in linked_ids.chunks(ID_QUERY_CHUNK) {
                let jql = format!("id in ({})", chunk.join(", "));
                others.extend(client.search(&jql).await?);
            }
            (JiraSyncMode::Full, mine, others)
        }
        SyncPlan::Delta { minutes } => {
            let changed = format!("{projects} AND updated >= \"-{minutes}m\"");
            let mine = client.search(&format!("{changed} AND {MINE}")).await?;
            // `!=` never matches an empty field, so unassigned issues are asked for explicitly.
            let others = client
                .search(&format!(
                    "{changed} AND (assignee != currentUser() OR assignee IS EMPTY \
                     OR statusCategory = Done)"
                ))
                .await?;
            (JiraSyncMode::Delta, mine, others)
        }
    };

    let mut statuses = Vec::new();
    for key in project_keys {
        statuses.extend(client.project_statuses(key).await?);
    }

    let result = SyncResult {
        mode,
        mine,
        others,
        statuses,
    };
    apply(&mut db.lock(), &result, &scope, now_ms)
}

/// Statuses of the synced projects plus the user's selection.
/// Done statuses are left out (done issues aren't kept, except in workstreams), as are
/// selected statuses that no longer exist in a project.
pub fn status_filter(conn: &Connection) -> AppResult<JiraStatusFilter> {
    let options = conn
        .prepare(
            "SELECT project_key, status_id, name, category FROM jira_project_statuses
             WHERE category <> 'done'
             ORDER BY project_key,
                      CASE category WHEN 'new' THEN 0 WHEN 'indeterminate' THEN 1
                                    WHEN 'done' THEN 2 ELSE 3 END,
                      rowid",
        )?
        .query_map([], |row| {
            Ok(JiraProjectStatus {
                project_key: row.get(0)?,
                status_id: row.get(1)?,
                name: row.get(2)?,
                category: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let selected = conn
        .prepare(
            "SELECT project_key, status_id FROM jira_status_filter
             JOIN jira_project_statuses USING (project_key, status_id)
             WHERE category <> 'done'
             ORDER BY project_key, status_id",
        )?
        .query_map([], |row| {
            Ok(JiraStatusRef {
                project_key: row.get(0)?,
                status_id: row.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(JiraStatusFilter { options, selected })
}

/// Replaces the user's status selection. An empty selection disables the filter.
pub fn set_status_filter(
    conn: &mut Connection,
    selected: &[JiraStatusRef],
) -> AppResult<JiraStatusFilter> {
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM jira_status_filter", [])?;
    {
        let mut insert = tx.prepare(
            "INSERT OR IGNORE INTO jira_status_filter (project_key, status_id) VALUES (?1, ?2)",
        )?;
        for status in selected {
            insert.execute(params![status.project_key, status.status_id])?;
        }
    }
    tx.commit()?;
    status_filter(conn)
}

/// Decides between a full and a delta sync.
fn plan(state: Option<&SyncState>, scope: &str, force_full: bool, now_ms: i64) -> SyncPlan {
    match state {
        Some(state) if !force_full && state.scope == scope => {
            let elapsed_ms = (now_ms - state.last_synced_at).max(0);
            let minutes = (elapsed_ms + 59_999) / 60_000 + DELTA_MARGIN_MINUTES;
            SyncPlan::Delta { minutes }
        }
        _ => SyncPlan::Full,
    }
}

/// Identifies what the stored issues belong to. If it changes, a delta is meaningless.
fn scope(client: &JiraClient, project_keys: &[String]) -> String {
    let mut keys = project_keys.to_vec();
    keys.sort();
    format!(
        "{}|{}|{}",
        client.base_url(),
        client.email(),
        keys.join(",")
    )
}

/// `project in ("FOO", "BAR")`. Keys are validated on save, so quoting is enough.
fn jql_projects(project_keys: &[String]) -> String {
    let quoted: Vec<String> = project_keys
        .iter()
        .map(|key| format!("\"{key}\""))
        .collect();
    format!("project in ({})", quoted.join(", "))
}

/// Everything fetched from Jira in one sync.
struct SyncResult {
    mode: JiraSyncMode,
    /// Issues assigned to the user.
    mine: Vec<JiraIssue>,
    /// Issues not assigned to the user: kept if linked to a workstream, otherwise pruned.
    others: Vec<JiraIssue>,
    statuses: Vec<JiraProjectStatus>,
}

/// Ids of all Jira issues linked to any workstream.
fn linked_issue_ids(conn: &Connection) -> AppResult<Vec<String>> {
    Ok(conn
        .prepare("SELECT item_id FROM workstream_items WHERE item_type = 'jira_issue'")?
        .query_map([], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?)
}

fn assigned_ids(conn: &Connection) -> AppResult<HashSet<String>> {
    Ok(conn
        .prepare("SELECT id FROM jira_issues WHERE assigned_to_me = 1")?
        .query_map([], |row| row.get(0))?
        .collect::<Result<HashSet<_>, _>>()?)
}

/// Writes a sync result in one transaction, so a failure leaves the previous data intact.
fn apply(
    conn: &mut Connection,
    result: &SyncResult,
    scope: &str,
    now_ms: i64,
) -> AppResult<JiraSyncSummary> {
    let tx = conn.transaction()?;
    let assigned_before = assigned_ids(&tx)?;
    if result.mode == JiraSyncMode::Full {
        // Whatever the full sync doesn't report as assigned is no longer assigned.
        tx.execute("UPDATE jira_issues SET assigned_to_me = 0", [])?;
    }
    {
        let mut upsert = tx.prepare(
            "INSERT INTO jira_issues
                 (id, key, summary, issue_type, updated, project_key, status_id, status_name,
                  assigned_to_me)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET
                 key = excluded.key, summary = excluded.summary,
                 issue_type = excluded.issue_type, updated = excluded.updated,
                 project_key = excluded.project_key, status_id = excluded.status_id,
                 status_name = excluded.status_name, assigned_to_me = excluded.assigned_to_me",
        )?;
        let tagged = (result.mine.iter().map(|issue| (issue, true)))
            .chain(result.others.iter().map(|issue| (issue, false)));
        for (issue, assigned) in tagged {
            upsert.execute(params![
                issue.id,
                issue.key,
                issue.summary,
                issue.issue_type,
                issue.updated,
                issue.project_key,
                issue.status_id,
                issue.status_name,
                assigned
            ])?;
        }
    }
    // Statuses are always fetched in full, so replace them wholesale.
    tx.execute("DELETE FROM jira_project_statuses", [])?;
    {
        let mut insert = tx.prepare(
            "INSERT OR IGNORE INTO jira_project_statuses (project_key, status_id, name, category)
             VALUES (?1, ?2, ?3, ?4)",
        )?;
        for status in &result.statuses {
            insert.execute(params![
                status.project_key,
                status.status_id,
                status.name,
                status.category
            ])?;
        }
    }
    // Done issues leave the list, also ones cached before done issues were skipped;
    // pruning then keeps only those in a workstream.
    tx.execute(
        "UPDATE jira_issues SET assigned_to_me = 0
         WHERE (project_key, status_id) IN
             (SELECT project_key, status_id FROM jira_project_statuses WHERE category = 'done')",
        [],
    )?;
    prune_unlinked(&tx)?;
    // "Removed" from the user's point of view: no longer in their list of assigned issues.
    let assigned_after = assigned_ids(&tx)?;
    let removed = assigned_before.difference(&assigned_after).count();
    tx.execute(
        "INSERT INTO jira_sync_state (id, scope, last_synced_at) VALUES (1, ?1, ?2)
         ON CONFLICT(id) DO UPDATE SET scope = excluded.scope, last_synced_at = excluded.last_synced_at",
        params![scope, now_ms],
    )?;
    tx.commit()?;
    Ok(JiraSyncSummary {
        mode: result.mode,
        upserted: result.mine.len(),
        removed,
        pull_request_error: None,
    })
}

fn load_state(conn: &Connection) -> AppResult<Option<SyncState>> {
    Ok(conn
        .query_row(
            "SELECT scope, last_synced_at FROM jira_sync_state WHERE id = 1",
            [],
            |row| {
                Ok(SyncState {
                    scope: row.get(0)?,
                    last_synced_at: row.get(1)?,
                })
            },
        )
        .optional()?)
}

#[cfg(test)]
mod tests {
    use mockito::{Matcher, Server};

    use super::*;
    use crate::db;
    use crate::services::jira_client::tests::{SEARCH, client};

    const NOW: i64 = 1_800_000_000_000;

    fn keys(keys: &[&str]) -> Vec<String> {
        keys.iter().map(|k| k.to_string()).collect()
    }

    fn issue_json(id: &str, key: &str, summary: &str) -> String {
        format!(
            r#"{{"id":"{id}","key":"{key}","fields":{{"summary":"{summary}","issuetype":{{"name":"Epic"}},"updated":"2026-11-02T10:00:00.000+0000","status":{{"id":"3","name":"In Progress"}}}}}}"#
        )
    }

    /// Every project gets the same workflow; listed out of category order on purpose.
    async fn mock_statuses(server: &mut Server) {
        server
            .mock(
                "GET",
                Matcher::Regex(r"^/rest/api/2/project/[A-Z_0-9]+/statuses$".into()),
            )
            .with_body(
                r#"[{"statuses":[
                    {"id":"5","name":"Done","statusCategory":{"key":"done"}},
                    {"id":"10","name":"To Do","statusCategory":{"key":"new"}},
                    {"id":"3","name":"In Progress","statusCategory":{"key":"indeterminate"}}
                ]}]"#,
            )
            .create_async()
            .await;
    }

    fn status(project_key: &str, status_id: &str) -> JiraStatusRef {
        JiraStatusRef {
            project_key: project_key.into(),
            status_id: status_id.into(),
        }
    }

    fn page(issues: &[String]) -> String {
        format!(r#"{{"issues":[{}],"isLast":true}}"#, issues.join(","))
    }

    async fn mock_search(server: &mut Server, jql_part: &str, body: String) -> mockito::Mock {
        server
            .mock("POST", SEARCH)
            .match_body(Matcher::Regex(regex_escape(jql_part)))
            .with_body(body)
            .create_async()
            .await
    }

    fn regex_escape(text: &str) -> String {
        text.chars()
            .flat_map(|c| {
                let escape = "\\.+*?()|[]{}^$".contains(c);
                escape.then_some('\\').into_iter().chain(Some(c))
            })
            .collect()
    }

    fn stored_keys(db: &Db) -> Vec<String> {
        list(&db.lock())
            .unwrap()
            .issues
            .into_iter()
            .map(|issue| issue.key)
            .collect()
    }

    #[test]
    fn plan_uses_delta_only_for_same_scope() {
        let state = SyncState {
            scope: "s".into(),
            last_synced_at: NOW - 10 * 60_000 - 1,
        };
        assert_eq!(
            plan(Some(&state), "s", false, NOW),
            SyncPlan::Delta {
                minutes: 11 + DELTA_MARGIN_MINUTES
            }
        );
        assert_eq!(plan(Some(&state), "other", false, NOW), SyncPlan::Full);
        assert_eq!(plan(Some(&state), "s", true, NOW), SyncPlan::Full);
        assert_eq!(plan(None, "s", false, NOW), SyncPlan::Full);
    }

    #[test]
    fn jql_projects_quotes_keys() {
        assert_eq!(
            jql_projects(&keys(&["FOO", "BAR"])),
            r#"project in ("FOO", "BAR")"#
        );
    }

    #[tokio::test]
    async fn first_sync_is_full_then_delta_applies_changes() {
        let mut server = Server::new_async().await;
        mock_statuses(&mut server).await;
        let db = Db::new(db::open_in_memory());
        let client = client(&server.url());
        let projects = keys(&["FOO"]);

        let full = mock_search(
            &mut server,
            "statusCategory != Done ORDER BY",
            page(&[
                issue_json("1", "FOO-1", "One"),
                issue_json("2", "FOO-2", "Two"),
            ]),
        )
        .await;
        let summary = sync(&db, &client, &projects, false, NOW).await.unwrap();
        assert_eq!(summary.mode, JiraSyncMode::Full);
        assert_eq!(summary.upserted, 2);
        assert_eq!(stored_keys(&db).len(), 2);
        full.assert_async().await;

        // 3 minutes later: FOO-1 was renamed, FOO-2 reassigned, FOO-3 newly assigned.
        server.reset();
        mock_statuses(&mut server).await;
        let mine = mock_search(
            &mut server,
            r#"updated >= \"-8m\" AND assignee = currentUser()"#,
            page(&[
                issue_json("1", "FOO-1", "One (renamed)"),
                issue_json("3", "FOO-3", "Three"),
            ]),
        )
        .await;
        let not_mine = mock_search(
            &mut server,
            "assignee IS EMPTY",
            page(&[issue_json("2", "FOO-2", "Two")]),
        )
        .await;
        let summary = sync(&db, &client, &projects, false, NOW + 3 * 60_000)
            .await
            .unwrap();
        assert_eq!(
            summary,
            JiraSyncSummary {
                mode: JiraSyncMode::Delta,
                upserted: 2,
                removed: 1,
                pull_request_error: None,
            }
        );
        mine.assert_async().await;
        not_mine.assert_async().await;

        let list = list(&db.lock()).unwrap();
        assert_eq!(list.last_synced_at, Some(NOW + 3 * 60_000));
        let mut summaries: Vec<_> = list.issues.iter().map(|i| i.summary.as_str()).collect();
        summaries.sort();
        assert_eq!(summaries, ["One (renamed)", "Three"]);
    }

    #[tokio::test]
    async fn changed_projects_force_full_sync_replacing_old_issues() {
        let mut server = Server::new_async().await;
        mock_statuses(&mut server).await;
        let db = Db::new(db::open_in_memory());
        let client = client(&server.url());

        mock_search(
            &mut server,
            r#"project in (\"FOO\")"#,
            page(&[issue_json("1", "FOO-1", "One")]),
        )
        .await;
        sync(&db, &client, &keys(&["FOO"]), false, NOW)
            .await
            .unwrap();

        mock_search(
            &mut server,
            r#"project in (\"BAR\")"#,
            page(&[issue_json("9", "BAR-9", "Nine")]),
        )
        .await;
        let summary = sync(&db, &client, &keys(&["BAR"]), false, NOW + 60_000)
            .await
            .unwrap();
        assert_eq!(summary.mode, JiraSyncMode::Full);
        assert_eq!(stored_keys(&db), ["BAR-9"]);
    }

    #[tokio::test]
    async fn failed_sync_keeps_existing_data() {
        let mut server = Server::new_async().await;
        mock_statuses(&mut server).await;
        let db = Db::new(db::open_in_memory());
        let client = client(&server.url());
        let projects = keys(&["FOO"]);

        mock_search(&mut server, "FOO", page(&[issue_json("1", "FOO-1", "One")])).await;
        sync(&db, &client, &projects, false, NOW).await.unwrap();

        server.reset();
        server
            .mock("POST", SEARCH)
            .with_status(500)
            .create_async()
            .await;
        assert!(
            sync(&db, &client, &projects, true, NOW + 60_000)
                .await
                .is_err()
        );
        let list = list(&db.lock()).unwrap();
        assert_eq!(list.issues.len(), 1);
        assert_eq!(list.last_synced_at, Some(NOW));
    }

    #[tokio::test]
    async fn sync_requires_project_keys() {
        let db = Db::new(db::open_in_memory());
        let client = client("http://127.0.0.1:9");
        assert!(matches!(
            sync(&db, &client, &[], false, NOW).await,
            Err(AppError::Validation(_))
        ));
    }

    #[tokio::test]
    async fn sync_stores_issue_status_type_and_project_statuses() {
        let mut server = Server::new_async().await;
        mock_statuses(&mut server).await;
        let db = Db::new(db::open_in_memory());
        mock_search(&mut server, "FOO", page(&[issue_json("1", "FOO-1", "One")])).await;
        sync(&db, &client(&server.url()), &keys(&["FOO"]), false, NOW)
            .await
            .unwrap();

        let issue = &list(&db.lock()).unwrap().issues[0];
        assert_eq!(
            (
                issue.project_key.as_str(),
                issue.status_id.as_str(),
                issue.status_name.as_str(),
                issue.issue_type.as_str()
            ),
            ("FOO", "3", "In Progress", "Epic")
        );
        let filter = status_filter(&db.lock()).unwrap();
        let names: Vec<_> = filter.options.iter().map(|s| s.name.as_str()).collect();
        // Done statuses can't match any listed issue.
        assert_eq!(names, ["To Do", "In Progress"]);
        assert!(filter.selected.is_empty());
    }

    #[test]
    fn status_filter_is_stored_and_ignores_unknown_statuses() {
        let mut conn = db::open_in_memory();
        conn.execute_batch(
            "INSERT INTO jira_project_statuses VALUES
                ('FOO', '3', 'In Progress', 'indeterminate'),
                ('BAR', '3', 'In Progress', 'indeterminate'),
                ('BAR', '5', 'Done', 'done');",
        )
        .unwrap();

        let filter = set_status_filter(
            &mut conn,
            &[status("BAR", "5"), status("FOO", "3"), status("FOO", "99")],
        )
        .unwrap();
        assert_eq!(filter.selected, [status("FOO", "3")]);
        let project_order: Vec<_> = filter
            .options
            .iter()
            .map(|s| s.project_key.as_str())
            .collect();
        assert_eq!(project_order, ["BAR", "FOO"]);

        // Replacing, not appending.
        let filter = set_status_filter(&mut conn, &[status("BAR", "3")]).unwrap();
        assert_eq!(filter.selected, [status("BAR", "3")]);
        assert!(
            set_status_filter(&mut conn, &[])
                .unwrap()
                .selected
                .is_empty()
        );
    }

    /// Creates workstream "W" and links Jira issue `id` to it.
    fn link_to_workstream(db: &Db, id: &str) -> i64 {
        use crate::models::workstreams::{WorkstreamItemRef, WorkstreamItemType};
        use crate::services::workstreams;
        let mut conn = db.lock();
        let workstream = workstreams::create(&conn, "W", NOW).unwrap();
        let item = WorkstreamItemRef {
            item_type: WorkstreamItemType::JiraIssue,
            item_id: id.into(),
        };
        workstreams::set_item_workstream(&mut conn, &item, Some(workstream.id), NOW).unwrap();
        workstream.id
    }

    #[tokio::test]
    async fn done_issues_are_dropped_unless_in_a_workstream() {
        let mut server = Server::new_async().await;
        mock_statuses(&mut server).await;
        let db = Db::new(db::open_in_memory());
        let client = client(&server.url());
        let projects = keys(&["FOO"]);
        let done = |id: &str, key: &str| {
            issue_json(id, key, "S").replace(
                r#""id":"3","name":"In Progress""#,
                r#""id":"5","name":"Done""#,
            )
        };

        let full = mock_search(
            &mut server,
            "statusCategory != Done ORDER BY",
            page(&[issue_json("1", "FOO-1", "S"), issue_json("2", "FOO-2", "S")]),
        )
        .await;
        sync(&db, &client, &projects, false, NOW).await.unwrap();
        full.assert_async().await;
        let workstream_id = link_to_workstream(&db, "2");

        // Both are done now; the delta lists them as no longer the user's.
        server.reset();
        mock_statuses(&mut server).await;
        mock_search(&mut server, "statusCategory != Done", page(&[])).await;
        mock_search(
            &mut server,
            "OR statusCategory = Done",
            page(&[done("1", "FOO-1"), done("2", "FOO-2")]),
        )
        .await;
        let summary = sync(&db, &client, &projects, false, NOW + 60_000)
            .await
            .unwrap();
        assert_eq!(summary.removed, 2);
        assert!(list(&db.lock()).unwrap().issues.is_empty());
        assert!(!exists(&db.lock(), "1").unwrap());
        let workstream = list_for_workstream(&db.lock(), workstream_id).unwrap();
        assert_eq!(
            (
                workstream[0].key.as_str(),
                workstream[0].status_name.as_str()
            ),
            ("FOO-2", "Done")
        );
    }

    #[tokio::test]
    async fn reassigned_issue_stays_in_its_workstream() {
        let mut server = Server::new_async().await;
        mock_statuses(&mut server).await;
        let db = Db::new(db::open_in_memory());
        let client = client(&server.url());
        let projects = keys(&["FOO"]);
        mock_search(
            &mut server,
            "ORDER BY",
            page(&[
                issue_json("1", "FOO-1", "One"),
                issue_json("2", "FOO-2", "Two"),
            ]),
        )
        .await;
        sync(&db, &client, &projects, false, NOW).await.unwrap();
        let workstream_id = link_to_workstream(&db, "2");

        // FOO-2 is handed to someone else and renamed; FOO-1 is unchanged.
        server.reset();
        mock_statuses(&mut server).await;
        mock_search(&mut server, "assignee = currentUser()", page(&[])).await;
        mock_search(
            &mut server,
            "assignee IS EMPTY",
            page(&[issue_json("2", "FOO-2", "Two (handed over)")]),
        )
        .await;
        let summary = sync(&db, &client, &projects, false, NOW + 60_000)
            .await
            .unwrap();
        assert_eq!(summary.removed, 1);

        assert_eq!(stored_keys(&db), ["FOO-1"]);
        let linked = list_for_workstream(&db.lock(), workstream_id).unwrap();
        assert_eq!(linked.len(), 1);
        assert_eq!(linked[0].summary, "Two (handed over)");
        assert!(!linked[0].assigned_to_me);
    }

    #[tokio::test]
    async fn full_sync_refreshes_linked_issues_by_id() {
        let mut server = Server::new_async().await;
        mock_statuses(&mut server).await;
        let db = Db::new(db::open_in_memory());
        let client = client(&server.url());
        let projects = keys(&["FOO"]);
        mock_search(
            &mut server,
            "ORDER BY",
            page(&[issue_json("7", "FOO-7", "Seven")]),
        )
        .await;
        sync(&db, &client, &projects, false, NOW).await.unwrap();
        let workstream_id = link_to_workstream(&db, "7");

        // FOO-7 is no longer assigned; the full sync finds it only through its id.
        server.reset();
        mock_statuses(&mut server).await;
        mock_search(&mut server, "ORDER BY", page(&[])).await;
        let by_id = mock_search(
            &mut server,
            "id in (7)",
            page(&[issue_json("7", "OTHER-7", "Seven (moved)")]),
        )
        .await;
        sync(&db, &client, &projects, true, NOW + 60_000)
            .await
            .unwrap();
        by_id.assert_async().await;

        assert!(stored_keys(&db).is_empty());
        let linked = list_for_workstream(&db.lock(), workstream_id).unwrap();
        assert_eq!(linked[0].key, "OTHER-7");
        assert!(!linked[0].assigned_to_me);
    }
}
