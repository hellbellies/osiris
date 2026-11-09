use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

use super::now_ms;
use crate::db::Db;
use crate::error::AppResult;
use crate::models::jira::{
    JiraIssueList, JiraSettings, JiraSettingsInput, JiraStatusFilter, JiraStatusRef,
    JiraSyncSummary, JiraUser,
};
use crate::services::jira;
use crate::services::jira_client::JiraClient;
use crate::services::jira_issues;
use crate::services::secrets::KeyringStore;

const TOKEN_STORE: KeyringStore = KeyringStore::new("ch.imfeld.osiris", "jira-api-token");

// Async so keychain, network and database work runs off the main thread.

#[tauri::command]
pub async fn get_jira_settings(app: AppHandle) -> AppResult<JiraSettings> {
    let dir = app.path().app_config_dir()?;
    jira::get(&dir, &TOKEN_STORE)
}

#[tauri::command]
pub async fn update_jira_settings(
    app: AppHandle,
    input: JiraSettingsInput,
) -> AppResult<JiraSettings> {
    let dir = app.path().app_config_dir()?;
    jira::save(&dir, &TOKEN_STORE, input)
}

#[tauri::command]
pub async fn clear_jira_token(app: AppHandle) -> AppResult<JiraSettings> {
    let dir = app.path().app_config_dir()?;
    jira::clear_token(&dir, &TOKEN_STORE)
}

/// Verifies the given (unsaved) settings against Jira without persisting them.
#[tauri::command]
pub async fn test_jira_connection(input: JiraSettingsInput) -> AppResult<JiraUser> {
    jira::test_connection(&TOKEN_STORE, input).await
}

#[tauri::command]
pub async fn list_jira_issues(db: State<'_, Db>) -> AppResult<JiraIssueList> {
    jira_issues::list(&db.lock())
}

/// Fetches the user's assigned issues; a delta sync unless `full` is set
/// or the site, account or project selection changed. Then updates the pull requests
/// related to them; a GitHub failure there is reported in the summary, not as an error.
#[tauri::command]
pub async fn sync_jira_issues(
    app: AppHandle,
    db: State<'_, Db>,
    full: bool,
) -> AppResult<JiraSyncSummary> {
    let dir = app.path().app_config_dir()?;
    let config = jira::load_config(&dir)?;
    let client = JiraClient::new(jira::credentials(&dir, &TOKEN_STORE)?)?;
    let mut summary = jira_issues::sync(&db, &client, &config.project_keys, full, now_ms()).await?;
    summary.pull_request_error = super::github::sync_jira_mentions(&app, &db, full)
        .await
        .err()
        .map(|err| err.to_string());
    Ok(summary)
}

#[tauri::command]
pub async fn get_jira_status_filter(db: State<'_, Db>) -> AppResult<JiraStatusFilter> {
    jira_issues::status_filter(&db.lock())
}

/// Stores which statuses to display; an empty list shows all issues.
#[tauri::command]
pub async fn set_jira_status_filter(
    db: State<'_, Db>,
    selected: Vec<JiraStatusRef>,
) -> AppResult<JiraStatusFilter> {
    jira_issues::set_status_filter(&mut db.lock(), &selected)
}

/// Opens an issue of the configured site in the default browser.
#[tauri::command]
pub async fn open_jira_issue(app: AppHandle, key: String) -> AppResult<()> {
    let dir = app.path().app_config_dir()?;
    let url = jira::issue_url(&dir, &key)?;
    app.opener().open_url(url, None::<&str>)?;
    Ok(())
}
