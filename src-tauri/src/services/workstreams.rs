//! Workstreams bundle related items of any type (Jira issues, pull requests, todos,
//! Obsidian notes).
//! A workstream is active until marked completed; completed ones keep their items.
//!
//! Links live in `workstream_items`, keyed by `(item_type, item_id)` so an item
//! belongs to at most one workstream. Removing a link may make a cached item
//! obsolete, so every unlink path asks the item's service to prune.

use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::error::{AppError, AppResult};
use crate::models::workstreams::{
    Workstream, WorkstreamDetail, WorkstreamItemRef, WorkstreamItemSearch, WorkstreamItemType,
};
use crate::services::{jira_issues, obsidian, pull_requests, todos};

const MAX_NAME_LENGTH: usize = 80;
/// Most results returned per item type by [`search_items`].
const SEARCH_LIMIT: i64 = 50;

/// Columns read by [`workstream_from_row`].
const COLUMNS: &str = "w.id, w.name, w.created_at, w.completed_at,
     (SELECT COUNT(*) FROM workstream_items i WHERE i.workstream_id = w.id)";

/// All workstreams with their item counts: active ones first, each group sorted by name.
pub fn list(conn: &Connection) -> AppResult<Vec<Workstream>> {
    Ok(conn
        .prepare(&format!(
            "SELECT {COLUMNS} FROM workstreams w
             ORDER BY w.completed_at IS NOT NULL, w.name COLLATE NOCASE"
        ))?
        .query_map([], workstream_from_row)?
        .collect::<Result<Vec<_>, _>>()?)
}

pub fn get(conn: &Connection, id: i64) -> AppResult<Workstream> {
    conn.query_row(
        &format!("SELECT {COLUMNS} FROM workstreams w WHERE w.id = ?1"),
        [id],
        workstream_from_row,
    )
    .optional()?
    .ok_or_else(not_found)
}

fn workstream_from_row(row: &Row<'_>) -> rusqlite::Result<Workstream> {
    Ok(Workstream {
        id: row.get(0)?,
        name: row.get(1)?,
        created_at: row.get(2)?,
        completed_at: row.get(3)?,
        item_count: row.get(4)?,
    })
}

/// A workstream with all its items.
pub fn detail(conn: &Connection, id: i64) -> AppResult<WorkstreamDetail> {
    Ok(WorkstreamDetail {
        workstream: get(conn, id)?,
        jira_issues: jira_issues::list_for_workstream(conn, id)?,
        pull_requests: pull_requests::list_for_workstream(conn, id)?,
        todos: todos::list_for_workstream(conn, id)?,
        obsidian_notes: obsidian::list_for_workstream(conn, id)?,
    })
}

pub fn create(conn: &Connection, name: &str, now_ms: i64) -> AppResult<Workstream> {
    let name = validate_name(conn, name, None)?;
    conn.execute(
        "INSERT INTO workstreams (name, created_at) VALUES (?1, ?2)",
        params![name, now_ms],
    )?;
    get(conn, conn.last_insert_rowid())
}

/// Creates a workstream and moves `items` into it, all or nothing.
pub fn create_with_items(
    conn: &mut Connection,
    name: &str,
    items: &[WorkstreamItemRef],
    now_ms: i64,
) -> AppResult<Workstream> {
    let tx = conn.transaction()?;
    let id = create(&tx, name, now_ms)?.id;
    for item in items {
        link(&tx, item, id, now_ms)?;
    }
    tx.commit()?;
    get(conn, id)
}

pub fn rename(conn: &Connection, id: i64, name: &str) -> AppResult<Workstream> {
    let name = validate_name(conn, name, Some(id))?;
    if conn.execute(
        "UPDATE workstreams SET name = ?1 WHERE id = ?2",
        params![name, id],
    )? == 0
    {
        return Err(not_found());
    }
    get(conn, id)
}

/// Marks a workstream completed (recording `now_ms`) or reopens it.
/// Completing an already completed workstream keeps the original date.
pub fn set_completed(
    conn: &Connection,
    id: i64,
    completed: bool,
    now_ms: i64,
) -> AppResult<Workstream> {
    let completed_at = completed.then_some(now_ms);
    if conn.execute(
        "UPDATE workstreams SET completed_at = CASE
             WHEN ?2 IS NULL THEN NULL ELSE COALESCE(completed_at, ?2) END
         WHERE id = ?1",
        params![id, completed_at],
    )? == 0
    {
        return Err(not_found());
    }
    get(conn, id)
}

/// Deletes a workstream. Its items are unlinked, not deleted.
pub fn delete(conn: &mut Connection, id: i64) -> AppResult<()> {
    let tx = conn.transaction()?;
    // `workstream_items` rows go with it via ON DELETE CASCADE.
    if tx.execute("DELETE FROM workstreams WHERE id = ?1", [id])? == 0 {
        return Err(not_found());
    }
    jira_issues::prune_unlinked(&tx)?;
    pull_requests::prune_unlinked(&tx)?;
    obsidian::prune_unlinked(&tx)?;
    tx.commit()?;
    Ok(())
}

/// Moves an item into `workstream_id`, or out of any workstream when `None`.
pub fn set_item_workstream(
    conn: &mut Connection,
    item: &WorkstreamItemRef,
    workstream_id: Option<i64>,
    now_ms: i64,
) -> AppResult<()> {
    let tx = conn.transaction()?;
    match workstream_id {
        Some(workstream_id) => {
            get(&tx, workstream_id)?;
            link(&tx, item, workstream_id, now_ms)?;
        }
        None => {
            tx.execute(
                "DELETE FROM workstream_items WHERE item_type = ?1 AND item_id = ?2",
                params![item.item_type.as_str(), item.item_id],
            )?;
            prune(&tx, item.item_type)?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// Links an existing item to `workstream_id`, moving it out of any other workstream.
fn link(
    conn: &Connection,
    item: &WorkstreamItemRef,
    workstream_id: i64,
    now_ms: i64,
) -> AppResult<()> {
    if !item_exists(conn, item)? {
        return Err(AppError::Validation("This item no longer exists".into()));
    }
    conn.execute(
        "INSERT INTO workstream_items (item_type, item_id, workstream_id, added_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (item_type, item_id) DO UPDATE SET
             workstream_id = excluded.workstream_id, added_at = excluded.added_at
         WHERE workstream_id != excluded.workstream_id",
        params![item.item_type.as_str(), item.item_id, workstream_id, now_ms],
    )?;
    Ok(())
}

/// Items of every type whose text contains `query` (trimmed, ASCII case-insensitive),
/// including items already in a workstream. A blank query matches nothing.
pub fn search_items(conn: &Connection, query: &str) -> AppResult<WorkstreamItemSearch> {
    let term = query.trim();
    if term.is_empty() {
        return Ok(WorkstreamItemSearch::default());
    }
    Ok(WorkstreamItemSearch {
        jira_issues: jira_issues::search(conn, term, SEARCH_LIMIT)?,
        pull_requests: pull_requests::search(conn, term, SEARCH_LIMIT)?,
        todos: todos::search(conn, term, SEARCH_LIMIT)?,
        ..WorkstreamItemSearch::default()
    })
}

fn item_exists(conn: &Connection, item: &WorkstreamItemRef) -> AppResult<bool> {
    match item.item_type {
        WorkstreamItemType::JiraIssue => jira_issues::exists(conn, &item.item_id),
        WorkstreamItemType::PullRequest => pull_requests::exists(conn, &item.item_id),
        WorkstreamItemType::Todo => todos::exists(conn, &item.item_id),
        // Notes live in Obsidian: linking one caches its vault and path.
        WorkstreamItemType::ObsidianNote => obsidian::register(conn, &item.item_id),
    }
}

/// Lets the item's own service drop cached items that were only kept for their link.
fn prune(conn: &Connection, item_type: WorkstreamItemType) -> AppResult<()> {
    match item_type {
        WorkstreamItemType::JiraIssue => jira_issues::prune_unlinked(conn).map(|_| ()),
        WorkstreamItemType::PullRequest => pull_requests::prune_unlinked(conn).map(|_| ()),
        // Todos belong to the user, not to a workstream: unlinking keeps them.
        WorkstreamItemType::Todo => Ok(()),
        WorkstreamItemType::ObsidianNote => obsidian::prune_unlinked(conn).map(|_| ()),
    }
}

/// Trims the name and checks it is non-empty, not too long and unique (ignoring case).
fn validate_name(conn: &Connection, name: &str, except_id: Option<i64>) -> AppResult<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::Validation("Enter a workstream name".into()));
    }
    if name.chars().count() > MAX_NAME_LENGTH {
        return Err(AppError::Validation(format!(
            "Workstream names can be at most {MAX_NAME_LENGTH} characters"
        )));
    }
    let taken = conn
        .query_row(
            "SELECT 1 FROM workstreams WHERE name = ?1 COLLATE NOCASE AND id IS NOT ?2",
            params![name, except_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if taken {
        return Err(AppError::Validation(format!(
            "A workstream named \"{name}\" already exists"
        )));
    }
    Ok(name.to_owned())
}

fn not_found() -> AppError {
    AppError::Validation("This workstream no longer exists".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    const NOW: i64 = 1_800_000_000_000;

    fn jira(id: &str) -> WorkstreamItemRef {
        WorkstreamItemRef {
            item_type: WorkstreamItemType::JiraIssue,
            item_id: id.into(),
        }
    }

    /// Caches Jira issue `id`, assigned to the user or not.
    fn insert_issue(conn: &Connection, id: &str, assigned: bool) {
        conn.execute(
            "INSERT INTO jira_issues (id, key, summary, updated, assigned_to_me)
             VALUES (?1, 'FOO-' || ?1, 'Summary', '2026-11-02', ?2)",
            params![id, assigned],
        )
        .unwrap();
    }

    #[test]
    fn obsidian_notes_are_cached_while_linked() {
        let mut conn = db::open_in_memory();
        let workstream = create(&conn, "W", NOW).unwrap();
        let note = WorkstreamItemRef {
            item_type: WorkstreamItemType::ObsidianNote,
            item_id: "Work/Meetings/Plan.md".into(),
        };
        set_item_workstream(&mut conn, &note, Some(workstream.id), NOW).unwrap();
        let notes = detail(&conn, workstream.id).unwrap().obsidian_notes;
        assert_eq!(
            (notes[0].title.as_str(), notes[0].workstream_id),
            ("Plan", Some(workstream.id))
        );

        set_item_workstream(&mut conn, &note, None, NOW).unwrap();
        assert!(
            detail(&conn, workstream.id)
                .unwrap()
                .obsidian_notes
                .is_empty()
        );
        let cached: i64 = conn
            .query_row("SELECT COUNT(*) FROM obsidian_notes", [], |row| row.get(0))
            .unwrap();
        assert_eq!(cached, 0);

        let bad = WorkstreamItemRef {
            item_id: "../etc/passwd".into(),
            ..note
        };
        assert!(set_item_workstream(&mut conn, &bad, Some(workstream.id), NOW).is_err());
    }

    #[test]
    fn create_validates_and_list_sorts_by_name() {
        let conn = db::open_in_memory();
        create(&conn, "  zeta ", NOW).unwrap();
        let alpha = create(&conn, "Alpha", NOW).unwrap();
        assert_eq!(alpha.item_count, 0);

        let names: Vec<_> = list(&conn).unwrap().into_iter().map(|w| w.name).collect();
        assert_eq!(names, ["Alpha", "zeta"]);

        for bad in ["", "   ", "ALPHA", &"x".repeat(MAX_NAME_LENGTH + 1)] {
            assert!(
                matches!(create(&conn, bad, NOW), Err(AppError::Validation(_))),
                "{bad}"
            );
        }
    }

    #[test]
    fn rename_allows_same_name_with_different_case_on_itself_only() {
        let conn = db::open_in_memory();
        let alpha = create(&conn, "Alpha", NOW).unwrap();
        create(&conn, "Beta", NOW).unwrap();
        assert_eq!(rename(&conn, alpha.id, "ALPHA").unwrap().name, "ALPHA");
        assert!(rename(&conn, alpha.id, "beta").is_err());
        assert!(rename(&conn, 999, "Gamma").is_err());
    }

    #[test]
    fn items_belong_to_one_workstream_and_can_move() {
        let mut conn = db::open_in_memory();
        insert_issue(&conn, "1", true);
        let a = create(&conn, "A", NOW).unwrap();
        let b = create(&conn, "B", NOW).unwrap();

        set_item_workstream(&mut conn, &jira("1"), Some(a.id), NOW).unwrap();
        set_item_workstream(&mut conn, &jira("1"), Some(b.id), NOW).unwrap();
        assert_eq!(get(&conn, a.id).unwrap().item_count, 0);
        let detail = detail(&conn, b.id).unwrap();
        assert_eq!(detail.workstream.item_count, 1);
        assert_eq!(detail.jira_issues[0].workstream_id, Some(b.id));

        // Shows up in the user's issue list with its workstream.
        let listed = jira_issues::list(&conn).unwrap().issues;
        assert_eq!(listed[0].workstream_id, Some(b.id));
    }

    #[test]
    fn set_item_workstream_rejects_unknown_item_or_workstream() {
        let mut conn = db::open_in_memory();
        insert_issue(&conn, "1", true);
        let a = create(&conn, "A", NOW).unwrap();
        assert!(set_item_workstream(&mut conn, &jira("404"), Some(a.id), NOW).is_err());
        assert!(set_item_workstream(&mut conn, &jira("1"), Some(999), NOW).is_err());
    }

    #[test]
    fn unlinking_prunes_issues_not_assigned_to_the_user() {
        let mut conn = db::open_in_memory();
        insert_issue(&conn, "mine", true);
        insert_issue(&conn, "theirs", false);
        let a = create(&conn, "A", NOW).unwrap();
        for id in ["mine", "theirs"] {
            set_item_workstream(&mut conn, &jira(id), Some(a.id), NOW).unwrap();
        }

        set_item_workstream(&mut conn, &jira("theirs"), None, NOW).unwrap();
        assert!(!jira_issues::exists(&conn, "theirs").unwrap());

        // Deleting the workstream unlinks the rest; assigned issues stay cached.
        delete(&mut conn, a.id).unwrap();
        assert!(jira_issues::exists(&conn, "mine").unwrap());
        assert!(list(&conn).unwrap().is_empty());
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM workstream_items", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    /// Caches pull request `id` in `repository`.
    fn insert_pull_request(
        conn: &Connection,
        id: &str,
        number: i64,
        title: &str,
        repository: &str,
    ) {
        conn.execute(
            "INSERT INTO github_pull_requests (id, number, title, url, repository, author, state,
                 is_draft, updated_at, authored_by_me, review_requested)
             VALUES (?1, ?2, ?3, 'https://github.com', ?4, 'me', 'open', 0, '2026-11-02', 1, 0)",
            params![id, number, title, repository],
        )
        .unwrap();
    }

    fn insert_todo(conn: &Connection, title: &str, description: &str) -> String {
        conn.execute(
            "INSERT INTO todos (title, description, created_at, updated_at) VALUES (?1, ?2, 0, 0)",
            params![title, description],
        )
        .unwrap();
        conn.last_insert_rowid().to_string()
    }

    #[test]
    fn search_items_matches_every_type_ignoring_case() {
        let conn = db::open_in_memory();
        insert_issue(&conn, "1", true); // FOO-1, "Summary"
        insert_issue(&conn, "2", false);
        insert_pull_request(&conn, "pr1", 42, "Checkout redesign", "acme/shop");
        insert_pull_request(&conn, "pr2", 7, "Bump deps", "acme/api");
        let todo = insert_todo(&conn, "Write spec", "for the CHECKOUT flow");
        insert_todo(&conn, "Unrelated", "");

        let found = search_items(&conn, "  checkout ").unwrap();
        assert!(found.jira_issues.is_empty());
        assert_eq!(found.pull_requests.len(), 1);
        assert_eq!(found.pull_requests[0].id, "pr1");
        assert_eq!(found.todos.len(), 1);
        assert_eq!(found.todos[0].id.to_string(), todo);

        // Jira keys, including issues not assigned to the user, and PR references.
        let ids: Vec<_> = search_items(&conn, "foo-")
            .unwrap()
            .jira_issues
            .into_iter()
            .map(|i| i.id)
            .collect();
        assert_eq!(ids.len(), 2);
        assert_eq!(
            search_items(&conn, "shop#42").unwrap().pull_requests.len(),
            1
        );
        // LIKE wildcards are matched literally.
        assert_eq!(
            search_items(&conn, "%").unwrap(),
            WorkstreamItemSearch::default()
        );
        assert_eq!(
            search_items(&conn, "   ").unwrap(),
            WorkstreamItemSearch::default()
        );
    }

    #[test]
    fn create_with_items_links_and_moves_items_atomically() {
        let mut conn = db::open_in_memory();
        insert_issue(&conn, "1", true);
        insert_pull_request(&conn, "pr1", 1, "Title", "acme/shop");
        let todo = insert_todo(&conn, "Todo", "");
        let old = create(&conn, "Old", NOW).unwrap();
        set_item_workstream(&mut conn, &jira("1"), Some(old.id), NOW).unwrap();

        let items = [
            jira("1"),
            WorkstreamItemRef {
                item_type: WorkstreamItemType::PullRequest,
                item_id: "pr1".into(),
            },
            WorkstreamItemRef {
                item_type: WorkstreamItemType::Todo,
                item_id: todo,
            },
        ];
        let new = create_with_items(&mut conn, "New", &items, NOW).unwrap();
        assert_eq!(new.item_count, 3);
        assert_eq!(get(&conn, old.id).unwrap().item_count, 0);

        // An unknown item rolls back the whole creation.
        let err = create_with_items(&mut conn, "Broken", &[jira("1"), jira("404")], NOW);
        assert!(matches!(err, Err(AppError::Validation(_))));
        assert_eq!(list(&conn).unwrap().len(), 2);
        assert_eq!(get(&conn, new.id).unwrap().item_count, 3);
    }

    #[test]
    fn completing_and_reopening() {
        let conn = db::open_in_memory();
        let alpha = create(&conn, "Alpha", NOW).unwrap();
        create(&conn, "Beta", NOW).unwrap();
        assert_eq!(alpha.completed_at, None);

        let done = set_completed(&conn, alpha.id, true, NOW + 1).unwrap();
        assert_eq!(done.completed_at, Some(NOW + 1));
        // Completing again keeps the original completion date.
        let again = set_completed(&conn, alpha.id, true, NOW + 2).unwrap();
        assert_eq!(again.completed_at, Some(NOW + 1));

        // Active workstreams are listed first.
        let names: Vec<_> = list(&conn).unwrap().into_iter().map(|w| w.name).collect();
        assert_eq!(names, ["Beta", "Alpha"]);

        assert_eq!(
            set_completed(&conn, alpha.id, false, NOW + 3)
                .unwrap()
                .completed_at,
            None
        );
        assert!(set_completed(&conn, 999, true, NOW).is_err());
    }
}
