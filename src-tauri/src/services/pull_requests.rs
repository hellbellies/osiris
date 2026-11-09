//! Local cache of GitHub pull requests and the sync that fills it.
//!
//! The cache holds open pull requests the user authored or was asked to review,
//! the other open pull requests in the watched repositories (the configured ones,
//! or else those of the user's own pull requests), plus pull requests linked to a
//! workstream (kept, and refreshed by id, after they are merged, closed or no
//! longer listed), plus pull requests in any state that mention a cached Jira
//! issue's key (searched in the same repositories). Anything else is pruned.
//!
//! Every sync is a full one: the user's open pull requests are few, so two or
//! three GraphQL requests replace the need for delta tracking. The exception are
//! Jira mentions: done issues are searched one last time, then keep their links
//! (refreshed by id), so a sync searches only for active issues.

use std::collections::{HashMap, HashSet};

use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::models::github::{
    GitHubSyncSummary, JiraIssuePullRequest, PullRequest, PullRequestList, PullRequestState,
    ReviewDecision,
};
use crate::services::github::{scope_qualifiers, search_scope};
use crate::services::github_client::GitHubClient;

/// Columns read by [`pull_request_from_row`]; `p` is `github_pull_requests`, `w` is `workstream_items`.
const COLUMNS: &str = "p.id, p.number, p.title, p.url, p.repository, p.author, p.state,
     p.is_draft, p.review_decision, p.updated_at, p.authored_by_me, p.review_requested,
     p.other_open, w.workstream_id";
const FROM: &str = "github_pull_requests p LEFT JOIN workstream_items w
     ON w.item_type = 'github_pr' AND w.item_id = p.id";
const INVOLVES_ME: &str = "(p.authored_by_me = 1 OR p.review_requested = 1)";
/// GitHub rejects search queries longer than this.
const MAX_QUERY_LENGTH: usize = 256;
/// Jira keys per search query: GitHub allows at most five `OR`s.
const KEYS_PER_QUERY: usize = 6;

/// Pull requests involving the user and other open ones in the watched
/// repositories, most recently updated first.
pub fn list(conn: &Connection) -> AppResult<PullRequestList> {
    let pull_requests = conn
        .prepare(&format!(
            "SELECT {COLUMNS} FROM {FROM} WHERE {INVOLVES_ME} OR p.other_open = 1
             ORDER BY p.updated_at DESC"
        ))?
        .query_map([], pull_request_from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    let last_synced_at = conn
        .query_row(
            "SELECT last_synced_at FROM github_sync_state WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .optional()?;
    Ok(PullRequestList {
        pull_requests,
        last_synced_at,
    })
}

/// Pull requests linked to a workstream, whatever their state.
pub fn list_for_workstream(conn: &Connection, workstream_id: i64) -> AppResult<Vec<PullRequest>> {
    Ok(conn
        .prepare(&format!(
            "SELECT {COLUMNS} FROM {FROM} WHERE w.workstream_id = ?1 ORDER BY p.updated_at DESC"
        ))?
        .query_map([workstream_id], pull_request_from_row)?
        .collect::<Result<Vec<_>, _>>()?)
}

/// Cached pull requests whose title or `owner/name#number` contains `term`
/// (ASCII case-insensitive), whatever their state, most recently updated first.
pub fn search(conn: &Connection, term: &str, limit: i64) -> AppResult<Vec<PullRequest>> {
    Ok(conn
        .prepare(&format!(
            "SELECT {COLUMNS} FROM {FROM}
             WHERE instr(lower(p.repository || '#' || p.number || ' ' || p.title), lower(?1)) > 0
             ORDER BY p.updated_at DESC LIMIT ?2"
        ))?
        .query_map(params![term, limit], pull_request_from_row)?
        .collect::<Result<Vec<_>, _>>()?)
}

pub fn exists(conn: &Connection, id: &str) -> AppResult<bool> {
    Ok(url(conn, id)?.is_some())
}

/// A cached pull request.
pub fn get(conn: &Connection, id: &str) -> AppResult<Option<PullRequest>> {
    Ok(conn
        .query_row(
            &format!("SELECT {COLUMNS} FROM {FROM} WHERE p.id = ?1"),
            [id],
            pull_request_from_row,
        )
        .optional()?)
}

/// Re-fetches one cached pull request by id and updates its details (state, title,
/// review decision, …). Why it is listed stays as the last sync determined.
pub async fn refresh(db: &Db, client: &GitHubClient, id: &str) -> AppResult<PullRequest> {
    let gone = || AppError::Validation("This pull request is no longer available".into());
    if !exists(&db.lock(), id)? {
        return Err(gone());
    }
    let pr = (client.pull_requests_by_id(&[id.to_owned()]).await?)
        .into_iter()
        .next()
        .ok_or_else(gone)?;
    let conn = db.lock();
    conn.execute(
        "UPDATE github_pull_requests
         SET number = ?2, title = ?3, url = ?4, repository = ?5, author = ?6, state = ?7,
             is_draft = ?8, review_decision = ?9, updated_at = ?10
         WHERE id = ?1",
        params![
            pr.id,
            pr.number,
            pr.title,
            pr.url,
            pr.repository,
            pr.author,
            state_to_db(pr.state),
            pr.is_draft,
            pr.review_decision.map(review_decision_to_db),
            pr.updated_at
        ],
    )?;
    get(&conn, id)?.ok_or_else(gone)
}

/// Pull requests mentioning each cached Jira issue, whatever their state,
/// most recently updated first.
pub fn list_for_jira_issues(conn: &Connection) -> AppResult<Vec<JiraIssuePullRequest>> {
    Ok(conn
        .prepare(&format!(
            "SELECT {COLUMNS}, l.issue_id FROM {FROM}
             JOIN jira_issue_pull_requests l ON l.pull_request_id = p.id
             ORDER BY p.updated_at DESC"
        ))?
        .query_map([], |row| {
            Ok(JiraIssuePullRequest {
                issue_id: row.get(14)?,
                pull_request: pull_request_from_row(row)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?)
}

/// Web URL of a cached pull request.
pub fn url(conn: &Connection, id: &str) -> AppResult<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT url FROM github_pull_requests WHERE id = ?1",
            [id],
            |row| row.get(0),
        )
        .optional()?)
}

/// Deletes pull requests that are neither listed, nor linked to a workstream,
/// nor mentioning a cached Jira issue.
pub fn prune_unlinked(conn: &Connection) -> AppResult<usize> {
    Ok(conn.execute(
        "DELETE FROM github_pull_requests
         WHERE authored_by_me = 0 AND review_requested = 0 AND other_open = 0
             AND id NOT IN (SELECT item_id FROM workstream_items WHERE item_type = 'github_pr')
             AND id NOT IN (SELECT pull_request_id FROM jira_issue_pull_requests)",
        [],
    )?)
}

/// Fetches open pull requests the user authored or must review (optionally limited
/// to `repositories`), the other open ones in the watched repositories, the ones
/// mentioning cached Jira issues, and refreshes workstream-linked ones.
pub async fn sync(
    db: &Db,
    client: &GitHubClient,
    repositories: &[String],
    now_ms: i64,
) -> AppResult<GitHubSyncSummary> {
    let scope = search_scope(repositories);
    let base = format!("is:pr is:open archived:false {scope}");
    let authored = client
        .search_pull_requests(&format!("{base} author:@me"))
        .await?;
    let requested = client
        .search_pull_requests(&format!("{base} review-requested:@me"))
        .await?;

    // One entry per pull request, flagged with every reason it involves the user.
    let mut mine: HashMap<String, PullRequest> = HashMap::new();
    for (pr, is_authored) in authored
        .into_iter()
        .map(|pr| (pr, true))
        .chain(requested.into_iter().map(|pr| (pr, false)))
    {
        let entry = mine.entry(pr.id.clone()).or_insert(pr);
        if is_authored {
            entry.authored_by_me = true;
        } else {
            entry.review_requested = true;
        }
    }

    // Watch the configured repositories, or else the ones the user works in.
    let watched = if repositories.is_empty() {
        let mut repos: Vec<String> = mine
            .values()
            .map(|pr| format!("repo:{}", pr.repository))
            .collect();
        repos.sort();
        repos.dedup();
        repos
    } else {
        scope_qualifiers(repositories)
    };
    let mut other_open: HashMap<String, PullRequest> = HashMap::new();
    let others_base = "is:pr is:open archived:false -author:@me -review-requested:@me";
    for query in batched_queries(others_base, &watched) {
        for mut pr in client.search_pull_requests(&query).await? {
            if !mine.contains_key(&pr.id) {
                pr.other_open = true;
                other_open.insert(pr.id.clone(), pr);
            }
        }
    }

    // Pull requests mentioning Jira issues, in any state. Without configured
    // repositories, keep searching where earlier mentions were found, so they don't
    // vanish once the user has no open pull request left in that repository.
    let mut mention_scope = watched;
    if repositories.is_empty() {
        mention_scope.extend(
            mentioned_repositories(&db.lock())?
                .into_iter()
                .map(|repo| format!("repo:{repo}")),
        );
        mention_scope.sort();
        mention_scope.dedup();
    }
    let searched: Vec<IssueToSearch> = issues_to_search(&db.lock(), &scope)?
        .into_iter()
        .filter(|issue| is_issue_key(&issue.key))
        .collect();
    let mut mentioning: HashMap<String, PullRequest> = HashMap::new();
    let mut mentions: Vec<(String, String)> = Vec::new();
    for chunk in searched.chunks(KEYS_PER_QUERY) {
        let terms: Vec<String> = chunk
            .iter()
            .map(|issue| format!("\"{}\"", issue.key))
            .collect();
        let base = format!("is:pr {}", terms.join(" OR "));
        for query in batched_queries(&base, &mention_scope) {
            for (pr, text) in client.search_pull_requests_with_text(&query).await? {
                let mut mentioned = false;
                for IssueToSearch {
                    id: issue_id, key, ..
                } in chunk
                {
                    if mentions_key(&text, key) {
                        mentions.push((issue_id.clone(), pr.id.clone()));
                        mentioned = true;
                    }
                }
                if mentioned && !mine.contains_key(&pr.id) && !other_open.contains_key(&pr.id) {
                    mentioning.insert(pr.id.clone(), pr);
                }
            }
        }
    }

    // Refresh by id what no search returned: workstream items, and the pull requests of
    // issues that weren't searched (their links are kept, but merges should show).
    let linked: Vec<String> = linked_ids(&db.lock())?
        .into_iter()
        .filter(|id| {
            !mine.contains_key(id) && !other_open.contains_key(id) && !mentioning.contains_key(id)
        })
        .collect();
    let linked = client.pull_requests_by_id(&linked).await?;

    let fetched = Fetched {
        mine: mine.into_values().collect(),
        other_open: other_open.into_values().collect(),
        mentioning: mentioning.into_values().collect(),
        linked,
        mentions,
        searched,
        scope,
    };
    apply(&mut db.lock(), &fetched, now_ms)
}

/// Everything one sync fetched from GitHub.
struct Fetched {
    mine: Vec<PullRequest>,
    other_open: Vec<PullRequest>,
    /// Pull requests mentioning a Jira issue that aren't in `mine` or `other_open`.
    mentioning: Vec<PullRequest>,
    /// Workstream-linked pull requests not found by any search, refreshed by id.
    linked: Vec<PullRequest>,
    /// `(Jira issue id, pull request id)` pairs found for the `searched` issues.
    mentions: Vec<(String, String)>,
    /// Jira issues whose mentions were searched; the others keep their links.
    searched: Vec<IssueToSearch>,
    /// The configured repository scope the issues were searched in.
    scope: String,
}

/// A Jira issue whose mentioning pull requests are searched.
struct IssueToSearch {
    id: String,
    key: String,
    /// Status category (`new`, `indeterminate`, `done`); empty if unknown.
    category: String,
}

/// Jira issues to search mentions for: all except done ones already searched while
/// done in the same configured repository `scope`. Pull requests rarely start
/// mentioning an issue after it is done, so this keeps a sync to a few searches.
fn issues_to_search(conn: &Connection, scope: &str) -> AppResult<Vec<IssueToSearch>> {
    Ok(conn
        .prepare(
            "SELECT i.id, i.key, coalesce(s.category, '') FROM jira_issues i
             LEFT JOIN jira_project_statuses s
                 ON s.project_key = i.project_key AND s.status_id = i.status_id
             LEFT JOIN jira_issue_pull_request_searches m ON m.issue_id = i.id
             WHERE coalesce(s.category, '') <> 'done'
                 OR m.category IS NOT 'done' OR m.scope IS NOT ?1
             ORDER BY i.key",
        )?
        .query_map([scope], |row| {
            Ok(IssueToSearch {
                id: row.get(0)?,
                key: row.get(1)?,
                category: row.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?)
}

/// Whether `key` looks like a Jira issue key (`ABC-123`), which also makes it safe
/// to put in a search query.
fn is_issue_key(key: &str) -> bool {
    let Some((project, number)) = key.split_once('-') else {
        return false;
    };
    project.starts_with(|c: char| c.is_ascii_alphabetic())
        && project
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !number.is_empty()
        && number.chars().all(|c| c.is_ascii_digit())
}

/// Whether `text` mentions `key` as a whole word, ignoring case: `FOO-1` matches
/// "Fix foo-1:" and "feature/FOO-1-login", but not "FOO-12" or "XFOO-1".
fn mentions_key(text: &str, key: &str) -> bool {
    let text = text.to_ascii_lowercase();
    let key = key.to_ascii_lowercase();
    let bytes = text.as_bytes();
    text.match_indices(&key).any(|(start, _)| {
        let end = start + key.len();
        let before_ok = start == 0 || !bytes[start - 1].is_ascii_alphanumeric();
        let after_ok = end == bytes.len() || !bytes[end].is_ascii_digit();
        before_ok && after_ok
    })
}

/// `base` plus as many `qualifiers` per query as fit in [`MAX_QUERY_LENGTH`]
/// (multiple `repo:`/`user:` qualifiers are ORed). None without qualifiers.
fn batched_queries(base: &str, qualifiers: &[String]) -> Vec<String> {
    let mut queries = Vec::new();
    let mut query = base.to_owned();
    for qualifier in qualifiers {
        if query.len() > base.len() && query.len() + 1 + qualifier.len() > MAX_QUERY_LENGTH {
            queries.push(std::mem::replace(&mut query, base.to_owned()));
        }
        query.push(' ');
        query.push_str(qualifier);
    }
    if query.len() > base.len() {
        queries.push(query);
    }
    queries
}

/// Writes a sync in one transaction, so a failure leaves the previous data intact.
fn apply(conn: &mut Connection, fetched: &Fetched, now_ms: i64) -> AppResult<GitHubSyncSummary> {
    let tx = conn.transaction()?;
    let before = involved_ids(&tx)?;
    // Whatever this sync doesn't list is no longer listed.
    tx.execute(
        "UPDATE github_pull_requests
         SET authored_by_me = 0, review_requested = 0, other_open = 0",
        [],
    )?;
    {
        let mut upsert = tx.prepare(
            "INSERT INTO github_pull_requests
                 (id, number, title, url, repository, author, state, is_draft,
                  review_decision, updated_at, authored_by_me, review_requested, other_open)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(id) DO UPDATE SET
                 number = excluded.number, title = excluded.title, url = excluded.url,
                 repository = excluded.repository, author = excluded.author,
                 state = excluded.state, is_draft = excluded.is_draft,
                 review_decision = excluded.review_decision, updated_at = excluded.updated_at,
                 authored_by_me = excluded.authored_by_me,
                 review_requested = excluded.review_requested,
                 other_open = excluded.other_open",
        )?;
        let all = (fetched.mine.iter())
            .chain(&fetched.other_open)
            .chain(&fetched.mentioning)
            .chain(&fetched.linked);
        for pr in all {
            upsert.execute(params![
                pr.id,
                pr.number,
                pr.title,
                pr.url,
                pr.repository,
                pr.author,
                state_to_db(pr.state),
                pr.is_draft,
                pr.review_decision.map(review_decision_to_db),
                pr.updated_at,
                pr.authored_by_me,
                pr.review_requested,
                pr.other_open
            ])?;
        }
    }
    {
        // Searched issues get their links replaced and are marked as searched.
        let mut unlink = tx.prepare("DELETE FROM jira_issue_pull_requests WHERE issue_id = ?1")?;
        // Skips issues a concurrent Jira sync removed in the meantime.
        let mut mark = tx.prepare(
            "INSERT INTO jira_issue_pull_request_searches (issue_id, category, scope)
             SELECT ?1, ?2, ?3 WHERE EXISTS (SELECT 1 FROM jira_issues WHERE id = ?1)
             ON CONFLICT(issue_id) DO UPDATE SET
                 category = excluded.category, scope = excluded.scope",
        )?;
        for issue in &fetched.searched {
            unlink.execute([&issue.id])?;
            mark.execute(params![issue.id, issue.category, fetched.scope])?;
        }
        let mut link = tx.prepare(
            "INSERT OR IGNORE INTO jira_issue_pull_requests (issue_id, pull_request_id)
             SELECT ?1, ?2 WHERE EXISTS (SELECT 1 FROM jira_issues WHERE id = ?1)",
        )?;
        for (issue_id, pull_request_id) in &fetched.mentions {
            link.execute(params![issue_id, pull_request_id])?;
        }
    }
    prune_unlinked(&tx)?;
    let after = involved_ids(&tx)?;
    tx.execute(
        "INSERT INTO github_sync_state (id, last_synced_at) VALUES (1, ?1)
         ON CONFLICT(id) DO UPDATE SET last_synced_at = excluded.last_synced_at",
        [now_ms],
    )?;
    tx.commit()?;
    Ok(GitHubSyncSummary {
        fetched: fetched.mine.len(),
        removed: before.difference(&after).count(),
        others: fetched.other_open.len(),
    })
}

/// Pull requests linked to a workstream or a Jira issue.
fn linked_ids(conn: &Connection) -> AppResult<Vec<String>> {
    Ok(conn
        .prepare(
            "SELECT item_id FROM workstream_items WHERE item_type = 'github_pr'
             UNION SELECT pull_request_id FROM jira_issue_pull_requests",
        )?
        .query_map([], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?)
}

/// Repositories of the pull requests currently linked to Jira issues.
fn mentioned_repositories(conn: &Connection) -> AppResult<Vec<String>> {
    Ok(conn
        .prepare(
            "SELECT DISTINCT p.repository FROM jira_issue_pull_requests l
             JOIN github_pull_requests p ON p.id = l.pull_request_id",
        )?
        .query_map([], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?)
}

fn involved_ids(conn: &Connection) -> AppResult<HashSet<String>> {
    Ok(conn
        .prepare(&format!(
            "SELECT id FROM github_pull_requests p WHERE {INVOLVES_ME}"
        ))?
        .query_map([], |row| row.get(0))?
        .collect::<Result<HashSet<_>, _>>()?)
}

fn pull_request_from_row(row: &Row<'_>) -> rusqlite::Result<PullRequest> {
    let state: String = row.get(6)?;
    let review_decision: Option<String> = row.get(8)?;
    Ok(PullRequest {
        id: row.get(0)?,
        number: row.get(1)?,
        title: row.get(2)?,
        url: row.get(3)?,
        repository: row.get(4)?,
        author: row.get(5)?,
        state: state_from_db(&state),
        is_draft: row.get(7)?,
        review_decision: review_decision.as_deref().and_then(review_decision_from_db),
        updated_at: row.get(9)?,
        authored_by_me: row.get(10)?,
        review_requested: row.get(11)?,
        other_open: row.get(12)?,
        workstream_id: row.get(13)?,
    })
}

fn state_to_db(state: PullRequestState) -> &'static str {
    match state {
        PullRequestState::Open => "open",
        PullRequestState::Closed => "closed",
        PullRequestState::Merged => "merged",
    }
}

fn state_from_db(value: &str) -> PullRequestState {
    match value {
        "merged" => PullRequestState::Merged,
        "closed" => PullRequestState::Closed,
        _ => PullRequestState::Open,
    }
}

fn review_decision_to_db(decision: ReviewDecision) -> &'static str {
    match decision {
        ReviewDecision::Approved => "approved",
        ReviewDecision::ChangesRequested => "changes_requested",
        ReviewDecision::ReviewRequired => "review_required",
    }
}

fn review_decision_from_db(value: &str) -> Option<ReviewDecision> {
    match value {
        "approved" => Some(ReviewDecision::Approved),
        "changes_requested" => Some(ReviewDecision::ChangesRequested),
        "review_required" => Some(ReviewDecision::ReviewRequired),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use mockito::{Matcher, Server};

    use super::*;
    use crate::db;
    use crate::models::workstreams::{WorkstreamItemRef, WorkstreamItemType};
    use crate::services::github_client::tests::{client, pr_json, search_page};
    use crate::services::workstreams;

    const NOW: i64 = 1_800_000_000_000;

    /// Mocks the two searches (matched by their `@me` qualifier) and the by-id lookup.
    async fn mock_github(
        server: &mut Server,
        authored: &[String],
        requested: &[String],
        by_id: &[String],
    ) {
        mock_github_with_others(server, authored, requested, &[], by_id).await;
    }

    /// Like [`mock_github`], plus the search for other open pull requests, which
    /// must be scoped to `acme/app` (the repository of every `pr_json`).
    async fn mock_github_with_others(
        server: &mut Server,
        authored: &[String],
        requested: &[String],
        others: &[String],
        by_id: &[String],
    ) {
        server.reset();
        // The user's own searches end with their `@me` qualifier; the other search
        // excludes both (`-author:@me -review-requested:@me`).
        server
            .mock("POST", "/graphql")
            .match_body(Matcher::Regex(r#"[^-]author:@me""#.into()))
            .with_body(search_page(authored, None))
            .create_async()
            .await;
        server
            .mock("POST", "/graphql")
            .match_body(Matcher::Regex(r#"[^-]review-requested:@me""#.into()))
            .with_body(search_page(requested, None))
            .create_async()
            .await;
        server
            .mock("POST", "/graphql")
            .match_body(Matcher::Regex(
                r#"-review-requested:@me repo:acme/app""#.into(),
            ))
            .with_body(search_page(others, None))
            .create_async()
            .await;
        server
            .mock("POST", "/graphql")
            .match_body(Matcher::Regex("nodes\\(ids".into()))
            .with_body(format!(r#"{{"data":{{"nodes":[{}]}}}}"#, by_id.join(",")))
            .create_async()
            .await;
    }

    fn listed_ids(db: &Db) -> Vec<String> {
        let mut ids: Vec<_> = list(&db.lock())
            .unwrap()
            .pull_requests
            .into_iter()
            .map(|pr| pr.id)
            .collect();
        ids.sort();
        ids
    }

    #[tokio::test]
    async fn sync_merges_authored_and_review_requested() {
        let mut server = Server::new_async().await;
        let db = Db::new(db::open_in_memory());
        mock_github(
            &mut server,
            &[pr_json("PR_1", 1, "OPEN"), pr_json("PR_2", 2, "OPEN")],
            &[pr_json("PR_2", 2, "OPEN"), pr_json("PR_3", 3, "OPEN")],
            &[],
        )
        .await;

        let summary = sync(&db, &client(&server.url()), &[], NOW).await.unwrap();
        assert_eq!(summary.fetched, 3);

        let list = list(&db.lock()).unwrap();
        assert_eq!(list.last_synced_at, Some(NOW));
        let pr2 = list
            .pull_requests
            .iter()
            .find(|pr| pr.id == "PR_2")
            .unwrap();
        assert!(pr2.authored_by_me && pr2.review_requested);
        let pr3 = list
            .pull_requests
            .iter()
            .find(|pr| pr.id == "PR_3")
            .unwrap();
        assert!(!pr3.authored_by_me && pr3.review_requested);
    }

    #[tokio::test]
    async fn merged_pull_request_stays_in_its_workstream() {
        let mut server = Server::new_async().await;
        let db = Db::new(db::open_in_memory());
        let github = client(&server.url());
        mock_github(
            &mut server,
            &[pr_json("PR_1", 1, "OPEN"), pr_json("PR_2", 2, "OPEN")],
            &[],
            &[],
        )
        .await;
        sync(&db, &github, &[], NOW).await.unwrap();

        let workstream_id = {
            let mut conn = db.lock();
            let workstream = workstreams::create(&conn, "W", NOW).unwrap();
            let item = WorkstreamItemRef {
                item_type: WorkstreamItemType::PullRequest,
                item_id: "PR_1".into(),
            };
            workstreams::set_item_workstream(&mut conn, &item, Some(workstream.id), NOW).unwrap();
            workstream.id
        };

        // Both get merged: no longer in the open-PR searches; PR_1 is refreshed by id.
        mock_github(&mut server, &[], &[], &[pr_json("PR_1", 1, "MERGED")]).await;
        let summary = sync(&db, &github, &[], NOW + 1).await.unwrap();
        assert_eq!(summary.removed, 2);

        assert!(listed_ids(&db).is_empty());
        assert!(!exists(&db.lock(), "PR_2").unwrap());
        let linked = list_for_workstream(&db.lock(), workstream_id).unwrap();
        assert_eq!(linked.len(), 1);
        assert_eq!(linked[0].state, PullRequestState::Merged);
    }

    #[tokio::test]
    async fn sync_uses_repository_scope_in_queries() {
        let mut server = Server::new_async().await;
        let db = Db::new(db::open_in_memory());
        let scoped = server
            .mock("POST", "/graphql")
            .match_body(Matcher::Regex("repo:acme/app user:octo".into()))
            .with_body(search_page(&[], None))
            // Authored, review requested, and the other open pull requests.
            .expect(3)
            .create_async()
            .await;
        server
            .mock("POST", "/graphql")
            .match_body(Matcher::Regex("nodes\\(ids".into()))
            .with_body(r#"{"data":{"nodes":[]}}"#)
            .create_async()
            .await;
        let repos = ["acme/app".to_string(), "octo".to_string()];
        sync(&db, &client(&server.url()), &repos, NOW)
            .await
            .unwrap();
        scoped.assert_async().await;
    }

    #[tokio::test]
    async fn sync_lists_other_open_pull_requests_in_the_users_repositories() {
        let mut server = Server::new_async().await;
        let db = Db::new(db::open_in_memory());
        let github = client(&server.url());
        // The other search may still return the user's own pull request; it stays "mine".
        mock_github_with_others(
            &mut server,
            &[pr_json("PR_1", 1, "OPEN")],
            &[],
            &[pr_json("PR_1", 1, "OPEN"), pr_json("PR_9", 9, "OPEN")],
            &[],
        )
        .await;
        let summary = sync(&db, &github, &[], NOW).await.unwrap();
        assert_eq!((summary.fetched, summary.others), (1, 1));

        let list = list(&db.lock()).unwrap().pull_requests;
        let flags = |id: &str| {
            let pr = list.iter().find(|pr| pr.id == id).unwrap();
            (pr.authored_by_me, pr.other_open)
        };
        assert_eq!(flags("PR_1"), (true, false));
        assert_eq!(flags("PR_9"), (false, true));

        // Once it is no longer open (or listed), it is pruned.
        mock_github(&mut server, &[pr_json("PR_1", 1, "OPEN")], &[], &[]).await;
        let summary = sync(&db, &github, &[], NOW + 1).await.unwrap();
        assert_eq!((summary.removed, summary.others), (0, 0));
        assert_eq!(listed_ids(&db), ["PR_1"]);
        assert!(!exists(&db.lock(), "PR_9").unwrap());
    }

    /// A pull request node with the text fields the Jira mention search asks for.
    fn pr_with_text(id: &str, number: i64, state: &str, branch: &str, body: &str) -> String {
        pr_json(id, number, state).replacen(
            '{',
            &format!(r#"{{"headRefName":"{branch}","bodyText":"{body}","#),
            1,
        )
    }

    /// Mocks the Jira mention search, the only one asking for `headRefName`.
    async fn mock_mentions(server: &mut Server, nodes: &[String]) {
        server
            .mock("POST", "/graphql")
            .match_body(Matcher::Regex("headRefName".into()))
            .with_body(search_page(nodes, None))
            .create_async()
            .await;
    }

    fn mentioned(db: &Db) -> Vec<(String, String, PullRequestState)> {
        let mut pairs: Vec<_> = list_for_jira_issues(&db.lock())
            .unwrap()
            .into_iter()
            .map(|m| (m.issue_id, m.pull_request.id, m.pull_request.state))
            .collect();
        pairs.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
        pairs
    }

    #[tokio::test]
    async fn sync_links_pull_requests_mentioning_jira_issues_in_any_state() {
        let mut server = Server::new_async().await;
        let db = Db::new(db::open_in_memory());
        let github = client(&server.url());
        for (id, key) in [("1", "FOO-1"), ("2", "FOO-2"), ("12", "FOO-12")] {
            db.lock()
                .execute(
                    "INSERT INTO jira_issues (id, key, summary, updated) VALUES (?1, ?2, 'S', '')",
                    params![id, key],
                )
                .unwrap();
        }

        // The user's own PR names FOO-2 in its branch; a merged PR names FOO-1 in its body
        // (and must not count for FOO-12); a search hit naming no key is ignored.
        mock_github(&mut server, &[pr_json("PR_1", 1, "OPEN")], &[], &[]).await;
        mock_mentions(
            &mut server,
            &[
                pr_with_text("PR_1", 1, "OPEN", "feature/foo-2-login", ""),
                pr_with_text("PR_5", 5, "MERGED", "main", "Fixes FOO-1."),
                pr_with_text("PR_7", 7, "CLOSED", "main", "Mentions FOO-123"),
            ],
        )
        .await;
        sync(&db, &github, &[], NOW).await.unwrap();
        assert_eq!(
            mentioned(&db),
            [
                ("1".into(), "PR_5".into(), PullRequestState::Merged),
                ("2".into(), "PR_1".into(), PullRequestState::Open),
            ]
        );
        assert!(!exists(&db.lock(), "PR_7").unwrap());
        // Mentioning pull requests are not listed on their own.
        assert_eq!(listed_ids(&db), ["PR_1"]);

        // No open PR of the user left: earlier mentions keep acme/app searched.
        mock_github(&mut server, &[], &[], &[]).await;
        mock_mentions(
            &mut server,
            &[pr_with_text("PR_5", 5, "MERGED", "main", "Fixes FOO-1.")],
        )
        .await;
        sync(&db, &github, &[], NOW + 1).await.unwrap();
        assert_eq!(
            mentioned(&db),
            [("1".into(), "PR_5".into(), PullRequestState::Merged)]
        );

        // No longer mentioned (or the issue is gone): link and pull request are pruned.
        mock_github(&mut server, &[], &[], &[]).await;
        mock_mentions(&mut server, &[]).await;
        sync(&db, &github, &[], NOW + 2).await.unwrap();
        assert!(mentioned(&db).is_empty());
        assert!(!exists(&db.lock(), "PR_5").unwrap());
    }

    #[tokio::test]
    async fn done_issues_keep_their_links_without_being_searched_again() {
        let mut server = Server::new_async().await;
        let db = Db::new(db::open_in_memory());
        let github = client(&server.url());
        {
            let conn = db.lock();
            conn.execute_batch(
                "INSERT INTO jira_project_statuses VALUES
                     ('FOO', '1', 'Open', 'new'), ('FOO', '9', 'Done', 'done');
                 INSERT INTO jira_issues (id, key, summary, updated, project_key, status_id)
                 VALUES ('1', 'FOO-1', 'S', '', 'FOO', '9'), ('2', 'FOO-2', 'S', '', 'FOO', '1');",
            )
            .unwrap();
        }
        let mine = [pr_json("PR_1", 1, "OPEN")];

        // First sync: both issues are searched.
        mock_github(&mut server, &mine, &[], &[]).await;
        mock_mentions(
            &mut server,
            &[
                pr_with_text("PR_5", 5, "OPEN", "main", "Fixes FOO-1."),
                pr_with_text("PR_6", 6, "OPEN", "main", "Part of FOO-2."),
            ],
        )
        .await;
        sync(&db, &github, &[], NOW).await.unwrap();

        // Second sync: only the active FOO-2 is searched; the done FOO-1 keeps its
        // pull request, refreshed by id (it was merged in the meantime).
        mock_github(&mut server, &mine, &[], &[pr_json("PR_5", 5, "MERGED")]).await;
        let done_search = server
            .mock("POST", "/graphql")
            .match_body(Matcher::Regex("FOO-1".into()))
            .expect(0)
            .create_async()
            .await;
        server
            .mock("POST", "/graphql")
            .match_body(Matcher::Regex("headRefName.*FOO-2".into()))
            .with_body(search_page(&[], None))
            .create_async()
            .await;
        sync(&db, &github, &[], NOW + 1).await.unwrap();
        done_search.assert_async().await;
        assert_eq!(
            mentioned(&db),
            [("1".into(), "PR_5".into(), PullRequestState::Merged)]
        );

        // Configuring repositories changes the scope: FOO-1 is searched once more.
        mock_github(&mut server, &mine, &[], &[]).await;
        let done_search = server
            .mock("POST", "/graphql")
            .match_body(Matcher::Regex("headRefName.*FOO-1".into()))
            .with_body(search_page(&[], None))
            .expect(1)
            .create_async()
            .await;
        sync(&db, &github, &["acme/app".into()], NOW + 2)
            .await
            .unwrap();
        done_search.assert_async().await;
        assert!(mentioned(&db).is_empty());
    }

    #[tokio::test]
    async fn refresh_updates_one_pull_request_and_keeps_its_listing() {
        let mut server = Server::new_async().await;
        let db = Db::new(db::open_in_memory());
        let github = client(&server.url());
        mock_github(&mut server, &[pr_json("PR_1", 1, "OPEN")], &[], &[]).await;
        sync(&db, &github, &[], NOW).await.unwrap();

        mock_github(&mut server, &[], &[], &[pr_json("PR_1", 1, "MERGED")]).await;
        let pr = refresh(&db, &github, "PR_1").await.unwrap();
        assert_eq!(pr.state, PullRequestState::Merged);
        assert!(pr.authored_by_me);
        assert_eq!(
            get(&db.lock(), "PR_1").unwrap().unwrap().state,
            PullRequestState::Merged
        );

        // Unknown locally, or no longer on GitHub.
        assert!(refresh(&db, &github, "PR_9").await.is_err());
        mock_github(&mut server, &[], &[], &[]).await;
        assert!(refresh(&db, &github, "PR_1").await.is_err());
    }

    #[test]
    fn mentions_key_matches_whole_keys_only() {
        assert!(mentions_key("Fix foo-1: login", "FOO-1"));
        assert!(mentions_key("feature/FOO-1-login", "FOO-1"));
        assert!(mentions_key("FOO-1", "FOO-1"));
        assert!(!mentions_key("FOO-12", "FOO-1"));
        assert!(!mentions_key("XFOO-1", "FOO-1"));
        assert!(mentions_key("see FOO-12 and FOO-1", "FOO-1"));
    }

    #[test]
    fn is_issue_key_accepts_jira_keys_only() {
        assert!(is_issue_key("FOO-1") && is_issue_key("A1_B-42"));
        for bad in ["FOO", "FOO-", "-1", "1FOO-1", "FOO-1a", "FOO-1\" OR"] {
            assert!(!is_issue_key(bad), "{bad}");
        }
    }

    #[test]
    fn batched_queries_keep_each_query_short_enough() {
        let base = "is:pr is:open";
        assert!(batched_queries(base, &[]).is_empty());
        assert_eq!(
            batched_queries(base, &["repo:a/b".into(), "user:c".into()]),
            ["is:pr is:open repo:a/b user:c"]
        );

        let qualifiers: Vec<String> = (0..40)
            .map(|i| format!("repo:acme/repository-{i}"))
            .collect();
        let queries = batched_queries(base, &qualifiers);
        assert!(queries.len() > 1);
        assert!(
            queries
                .iter()
                .all(|q| q.len() <= MAX_QUERY_LENGTH && q.starts_with(base))
        );
        let joined = queries.join(" ");
        assert!(qualifiers.iter().all(|q| joined.contains(q.as_str())));
    }

    #[tokio::test]
    async fn failed_sync_keeps_existing_data() {
        let mut server = Server::new_async().await;
        let db = Db::new(db::open_in_memory());
        let github = client(&server.url());
        mock_github(&mut server, &[pr_json("PR_1", 1, "OPEN")], &[], &[]).await;
        sync(&db, &github, &[], NOW).await.unwrap();

        server.reset();
        server
            .mock("POST", "/graphql")
            .with_status(502)
            .create_async()
            .await;
        assert!(sync(&db, &github, &[], NOW + 1).await.is_err());
        assert_eq!(listed_ids(&db), ["PR_1"]);
        assert_eq!(list(&db.lock()).unwrap().last_synced_at, Some(NOW));
    }
}
