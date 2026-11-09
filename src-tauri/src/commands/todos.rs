use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use super::now_ms;
use crate::db::Db;
use crate::error::AppResult;
use crate::models::todos::{Todo, TodoInput};
use crate::services::todos;

#[tauri::command]
pub async fn list_todos(db: State<'_, Db>) -> AppResult<Vec<Todo>> {
    todos::list(&db.lock())
}

/// Creates a todo; with `workstream_id` it is added to that workstream right away.
#[tauri::command]
pub async fn create_todo(
    db: State<'_, Db>,
    input: TodoInput,
    workstream_id: Option<i64>,
) -> AppResult<Todo> {
    todos::create(&mut db.lock(), &input, workstream_id, now_ms())
}

#[tauri::command]
pub async fn update_todo(db: State<'_, Db>, id: i64, input: TodoInput) -> AppResult<Todo> {
    todos::update(&db.lock(), id, &input, now_ms())
}

#[tauri::command]
pub async fn set_todo_completed(db: State<'_, Db>, id: i64, completed: bool) -> AppResult<Todo> {
    todos::set_completed(&db.lock(), id, completed, now_ms())
}

#[tauri::command]
pub async fn delete_todo(db: State<'_, Db>, id: i64) -> AppResult<()> {
    todos::delete(&mut db.lock(), id)
}

/// Opens a link from a todo description in the default browser or mail app.
#[tauri::command]
pub async fn open_todo_link(app: AppHandle, url: String) -> AppResult<()> {
    app.opener().open_url(todos::link_url(&url)?, None::<&str>)?;
    Ok(())
}
