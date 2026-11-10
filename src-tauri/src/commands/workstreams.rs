use tauri::{AppHandle, State};

use super::now_ms;
use crate::db::Db;
use crate::error::AppResult;
use crate::models::workstreams::{
    Workstream, WorkstreamDetail, WorkstreamItemRef, WorkstreamItemSearch,
};
use crate::services::{obsidian, workstreams};

#[tauri::command]
pub async fn list_workstreams(db: State<'_, Db>) -> AppResult<Vec<Workstream>> {
    workstreams::list(&db.lock())
}

#[tauri::command]
pub async fn get_workstream(db: State<'_, Db>, id: i64) -> AppResult<WorkstreamDetail> {
    workstreams::detail(&db.lock(), id)
}

/// Creates a workstream holding `items` (which may be empty), moving them out of other workstreams.
#[tauri::command]
pub async fn create_workstream(
    db: State<'_, Db>,
    name: String,
    items: Vec<WorkstreamItemRef>,
) -> AppResult<Workstream> {
    workstreams::create_with_items(&mut db.lock(), &name, &items, now_ms())
}

#[tauri::command]
pub async fn rename_workstream(db: State<'_, Db>, id: i64, name: String) -> AppResult<Workstream> {
    workstreams::rename(&db.lock(), id, &name)
}

/// Marks a workstream completed, or reopens it when `completed` is false.
#[tauri::command]
pub async fn set_workstream_completed(
    db: State<'_, Db>,
    id: i64,
    completed: bool,
) -> AppResult<Workstream> {
    workstreams::set_completed(&db.lock(), id, completed, now_ms())
}

#[tauri::command]
pub async fn delete_workstream(db: State<'_, Db>, id: i64) -> AppResult<()> {
    workstreams::delete(&mut db.lock(), id)
}

/// Assigns an item to a workstream, or removes it from its workstream when `workstream_id` is null.
#[tauri::command]
pub async fn set_item_workstream(
    db: State<'_, Db>,
    item: WorkstreamItemRef,
    workstream_id: Option<i64>,
) -> AppResult<()> {
    workstreams::set_item_workstream(&mut db.lock(), &item, workstream_id, now_ms())
}

/// Jira issues, pull requests, todos and Obsidian notes matching `query`. Obsidian
/// being unavailable is reported in the result, not as an error.
#[tauri::command]
pub async fn search_workstream_items(
    app: AppHandle,
    db: State<'_, Db>,
    query: String,
) -> AppResult<WorkstreamItemSearch> {
    let mut found = workstreams::search_items(&db.lock(), &query)?;
    let app = super::obsidian::app(&app)?;
    let limit = super::obsidian::SEARCH_LIMIT;
    match super::obsidian::blocking(move || obsidian::search(&app, &query, limit)).await {
        Ok(notes) => found.obsidian_notes = obsidian::with_workstreams(&db.lock(), &notes)?,
        Err(err) => found.obsidian_error = Some(err.to_string()),
    }
    Ok(found)
}
