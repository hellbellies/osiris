use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::models::obsidian::{ObsidianNote, ObsidianStatus};
use crate::services::obsidian::{self, ObsidianApp};

/// Most notes returned per vault by a search.
pub(super) const SEARCH_LIMIT: usize = 30;

pub(super) fn app(handle: &AppHandle) -> AppResult<ObsidianApp> {
    Ok(ObsidianApp::new(&handle.path().config_dir()?))
}

/// Runs blocking Obsidian work (CLI processes) off the async runtime.
pub(super) async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|err| AppError::Obsidian(format!("Obsidian check failed: {err}")))?
}

/// Which prerequisites for Obsidian notes are met. Never starts Obsidian.
#[tauri::command]
pub async fn get_obsidian_status(app: AppHandle) -> AppResult<ObsidianStatus> {
    let obsidian = self::app(&app)?;
    blocking(move || Ok(obsidian::status(&obsidian))).await
}

/// Notes in the open vaults matching `query`, with their workstream.
#[tauri::command]
pub async fn search_obsidian_notes(
    app: AppHandle,
    db: State<'_, Db>,
    query: String,
) -> AppResult<Vec<ObsidianNote>> {
    let obsidian = self::app(&app)?;
    let found = blocking(move || obsidian::search(&obsidian, &query, SEARCH_LIMIT)).await?;
    obsidian::with_workstreams(&db.lock(), &found)
}

/// Opens a note in Obsidian (launching it if needed).
#[tauri::command]
pub async fn open_obsidian_note(app: AppHandle, id: String) -> AppResult<()> {
    app.opener()
        .open_url(obsidian::open_url(&id)?, None::<&str>)?;
    Ok(())
}
