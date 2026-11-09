//! Todos: created in osiris itself, optionally linked to a workstream.
//!
//! Unlike Jira issues and pull requests they are never pruned: removing a todo
//! from its workstream keeps it. Deleting a todo also removes its link.

use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::error::{AppError, AppResult};
use crate::models::todos::{Todo, TodoInput};
use crate::services::workstreams;

const MAX_TITLE_LENGTH: usize = 200;
const MAX_DESCRIPTION_LENGTH: usize = 10_000;

/// Columns read by [`todo_from_row`]; `t` is `todos`, `w` is `workstream_items`.
const COLUMNS: &str =
    "t.id, t.title, t.description, t.created_at, t.updated_at, t.completed_at, w.workstream_id";
/// `workstream_items.item_id` is text, so compare against the todo id as text.
const FROM: &str = "todos t LEFT JOIN workstream_items w
     ON w.item_type = 'todo' AND w.item_id = CAST(t.id AS TEXT)";
/// Open todos first (oldest first, like a queue), then completed ones (most recent first).
const ORDER: &str = "t.completed_at IS NOT NULL, t.completed_at DESC, t.created_at, t.id";

/// Schemes a link in a todo description may open; anything else (e.g. `file:`) is refused.
/// Mirrored by `LINK_SCHEMES` in `TodoDescription.vue`.
const LINK_SCHEMES: [&str; 4] = ["http", "https", "mailto", "obsidian"];

/// Validates a link from a todo description before it is opened in the default handler.
pub fn link_url(link: &str) -> AppResult<url::Url> {
    url::Url::parse(link.trim())
        .ok()
        .filter(|url| LINK_SCHEMES.contains(&url.scheme()))
        .ok_or_else(|| AppError::Validation(format!("Cannot open link “{link}”")))
}

pub fn list(conn: &Connection) -> AppResult<Vec<Todo>> {
    Ok(conn
        .prepare(&format!("SELECT {COLUMNS} FROM {FROM} ORDER BY {ORDER}"))?
        .query_map([], todo_from_row)?
        .collect::<Result<Vec<_>, _>>()?)
}

pub fn list_for_workstream(conn: &Connection, workstream_id: i64) -> AppResult<Vec<Todo>> {
    Ok(conn
        .prepare(&format!(
            "SELECT {COLUMNS} FROM {FROM} WHERE w.workstream_id = ?1 ORDER BY {ORDER}"
        ))?
        .query_map([workstream_id], todo_from_row)?
        .collect::<Result<Vec<_>, _>>()?)
}

pub fn get(conn: &Connection, id: i64) -> AppResult<Todo> {
    conn.query_row(
        &format!("SELECT {COLUMNS} FROM {FROM} WHERE t.id = ?1"),
        [id],
        todo_from_row,
    )
    .optional()?
    .ok_or_else(not_found)
}

/// Todos whose title or description contains `term` (ASCII case-insensitive).
pub fn search(conn: &Connection, term: &str, limit: i64) -> AppResult<Vec<Todo>> {
    Ok(conn
        .prepare(&format!(
            "SELECT {COLUMNS} FROM {FROM}
             WHERE instr(lower(t.title || ' ' || t.description), lower(?1)) > 0
             ORDER BY {ORDER} LIMIT ?2"
        ))?
        .query_map(params![term, limit], todo_from_row)?
        .collect::<Result<Vec<_>, _>>()?)
}

/// Whether a todo exists; `id` is the text form used in `workstream_items`.
pub fn exists(conn: &Connection, id: &str) -> AppResult<bool> {
    let Ok(id) = id.parse::<i64>() else {
        return Ok(false);
    };
    Ok(conn
        .query_row("SELECT 1 FROM todos WHERE id = ?1", [id], |_| Ok(()))
        .optional()?
        .is_some())
}

/// Creates a todo, optionally directly inside a workstream.
pub fn create(
    conn: &mut Connection,
    input: &TodoInput,
    workstream_id: Option<i64>,
    now_ms: i64,
) -> AppResult<Todo> {
    let (title, description) = validate(input)?;
    let tx = conn.transaction()?;
    if let Some(workstream_id) = workstream_id {
        workstreams::get(&tx, workstream_id)?;
    }
    tx.execute(
        "INSERT INTO todos (title, description, created_at, updated_at) VALUES (?1, ?2, ?3, ?3)",
        params![title, description, now_ms],
    )?;
    let id = tx.last_insert_rowid();
    if let Some(workstream_id) = workstream_id {
        tx.execute(
            "INSERT INTO workstream_items (item_type, item_id, workstream_id, added_at)
             VALUES ('todo', ?1, ?2, ?3)",
            params![id.to_string(), workstream_id, now_ms],
        )?;
    }
    tx.commit()?;
    get(conn, id)
}

pub fn update(conn: &Connection, id: i64, input: &TodoInput, now_ms: i64) -> AppResult<Todo> {
    let (title, description) = validate(input)?;
    let changed = conn.execute(
        "UPDATE todos SET title = ?2, description = ?3, updated_at = ?4 WHERE id = ?1",
        params![id, title, description, now_ms],
    )?;
    if changed == 0 {
        return Err(not_found());
    }
    get(conn, id)
}

/// Marks a todo completed (keeping an earlier completion date) or reopens it.
pub fn set_completed(conn: &Connection, id: i64, completed: bool, now_ms: i64) -> AppResult<Todo> {
    let changed = conn.execute(
        "UPDATE todos SET completed_at = CASE
             WHEN ?2 IS NULL THEN NULL ELSE COALESCE(completed_at, ?2) END
         WHERE id = ?1",
        params![id, completed.then_some(now_ms)],
    )?;
    if changed == 0 {
        return Err(not_found());
    }
    get(conn, id)
}

/// Deletes a todo and its workstream link.
pub fn delete(conn: &mut Connection, id: i64) -> AppResult<()> {
    let tx = conn.transaction()?;
    tx.execute(
        "DELETE FROM workstream_items WHERE item_type = 'todo' AND item_id = ?1",
        [id.to_string()],
    )?;
    if tx.execute("DELETE FROM todos WHERE id = ?1", [id])? == 0 {
        return Err(not_found());
    }
    tx.commit()?;
    Ok(())
}

/// Trims title and description and enforces their limits.
fn validate(input: &TodoInput) -> AppResult<(String, String)> {
    let title = input.title.trim();
    if title.is_empty() {
        return Err(AppError::Validation("Enter a title".into()));
    }
    if title.chars().count() > MAX_TITLE_LENGTH {
        return Err(AppError::Validation(format!(
            "Titles can be at most {MAX_TITLE_LENGTH} characters"
        )));
    }
    let description = input.description.trim();
    if description.chars().count() > MAX_DESCRIPTION_LENGTH {
        return Err(AppError::Validation(format!(
            "Descriptions can be at most {MAX_DESCRIPTION_LENGTH} characters"
        )));
    }
    Ok((title.to_owned(), description.to_owned()))
}

fn todo_from_row(row: &Row<'_>) -> rusqlite::Result<Todo> {
    Ok(Todo {
        id: row.get(0)?,
        title: row.get(1)?,
        description: row.get(2)?,
        created_at: row.get(3)?,
        updated_at: row.get(4)?,
        completed_at: row.get(5)?,
        workstream_id: row.get(6)?,
    })
}

fn not_found() -> AppError {
    AppError::Validation("This todo no longer exists".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::models::workstreams::{WorkstreamItemRef, WorkstreamItemType};

    const NOW: i64 = 1_800_000_000_000;

    fn input(title: &str, description: &str) -> TodoInput {
        TodoInput {
            title: title.into(),
            description: description.into(),
        }
    }

    #[test]
    fn create_update_and_validate() {
        let mut conn = db::open_in_memory();
        let todo = create(&mut conn, &input("  Write spec ", " Draft\n"), None, NOW).unwrap();
        assert_eq!(
            (todo.title.as_str(), todo.description.as_str()),
            ("Write spec", "Draft")
        );
        assert_eq!(todo.workstream_id, None);

        let updated = update(&conn, todo.id, &input("Write the spec", ""), NOW + 1).unwrap();
        assert_eq!(updated.title, "Write the spec");
        assert_eq!(updated.updated_at, NOW + 1);
        assert_eq!(updated.created_at, NOW);

        for bad in [input(" ", ""), input(&"x".repeat(201), "")] {
            assert!(matches!(
                create(&mut conn, &bad, None, NOW),
                Err(AppError::Validation(_))
            ));
        }
        assert!(update(&conn, 999, &input("x", ""), NOW).is_err());
    }

    #[test]
    fn create_inside_workstream_and_complete() {
        let mut conn = db::open_in_memory();
        let workstream = workstreams::create(&conn, "W", NOW).unwrap();
        let first = create(&mut conn, &input("First", ""), Some(workstream.id), NOW).unwrap();
        let second = create(
            &mut conn,
            &input("Second", ""),
            Some(workstream.id),
            NOW + 1,
        )
        .unwrap();
        assert_eq!(first.workstream_id, Some(workstream.id));
        assert_eq!(
            workstreams::get(&conn, workstream.id).unwrap().item_count,
            2
        );
        assert!(create(&mut conn, &input("Orphan", ""), Some(999), NOW).is_err());

        set_completed(&conn, first.id, true, NOW + 5).unwrap();
        // Open todos come first.
        let titles: Vec<_> = list_for_workstream(&conn, workstream.id)
            .unwrap()
            .into_iter()
            .map(|t| t.title)
            .collect();
        assert_eq!(titles, ["Second", "First"]);
        assert_eq!(
            set_completed(&conn, first.id, false, NOW)
                .unwrap()
                .completed_at,
            None
        );
        assert_eq!(second.completed_at, None);
    }

    #[test]
    fn unlinking_keeps_the_todo_and_deleting_removes_the_link() {
        let mut conn = db::open_in_memory();
        let workstream = workstreams::create(&conn, "W", NOW).unwrap();
        let todo = create(&mut conn, &input("Keep me", ""), Some(workstream.id), NOW).unwrap();
        let item = WorkstreamItemRef {
            item_type: WorkstreamItemType::Todo,
            item_id: todo.id.to_string(),
        };

        workstreams::set_item_workstream(&mut conn, &item, None, NOW).unwrap();
        assert_eq!(get(&conn, todo.id).unwrap().workstream_id, None);

        workstreams::set_item_workstream(&mut conn, &item, Some(workstream.id), NOW).unwrap();
        delete(&mut conn, todo.id).unwrap();
        assert!(list(&conn).unwrap().is_empty());
        assert_eq!(
            workstreams::get(&conn, workstream.id).unwrap().item_count,
            0
        );
        assert!(delete(&mut conn, todo.id).is_err());
    }

    #[test]
    fn link_url_accepts_known_schemes_only() {
        assert_eq!(
            link_url(" https://example.com/a?b=1 ").unwrap().as_str(),
            "https://example.com/a?b=1"
        );
        assert!(link_url("http://example.com").is_ok());
        assert!(link_url("mailto:someone@example.com").is_ok());
        assert!(link_url("obsidian://open?vault=Notes&file=Meeting%20-%20Plan").is_ok());
        for link in ["file:///etc/passwd", "javascript:alert(1)", "/relative", "not a url", ""] {
            assert!(link_url(link).is_err(), "{link}");
        }
    }
}
