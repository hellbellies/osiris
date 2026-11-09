use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

use super::now_ms;
use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::models::github::{
    GitHubSettings, GitHubSettingsInput, GitHubSyncSummary, GitHubUser, JiraIssuePullRequest,
    PullRequest, PullRequestList,
};
use crate::services::github_client::GitHubClient;
use crate::services::secrets::{KeyringStore, SecretStore};
use crate::services::{github, pull_requests};

pub(super) const TOKEN_STORE: KeyringStore = KeyringStore::new("ch.imfeld.osiris", "github-token");

#[tauri::command]
pub async fn get_github_settings(app: AppHandle) -> AppResult<GitHubSettings> {
    let dir = app.path().app_config_dir()?;
    github::get(&dir, &TOKEN_STORE)
}

#[tauri::command]
pub async fn update_github_settings(
    app: AppHandle,
    input: GitHubSettingsInput,
) -> AppResult<GitHubSettings> {
    let dir = app.path().app_config_dir()?;
    github::save(&dir, &TOKEN_STORE, input)
}

#[tauri::command]
pub async fn clear_github_token(app: AppHandle) -> AppResult<GitHubSettings> {
    let dir = app.path().app_config_dir()?;
    github::clear_token(&dir, &TOKEN_STORE)
}

/// Verifies a token (or the stored one when `token` is null) without saving it.
#[tauri::command]
pub async fn test_github_connection(token: Option<String>) -> AppResult<GitHubUser> {
    github::test_connection(&TOKEN_STORE, token).await
}

#[tauri::command]
pub async fn list_pull_requests(db: State<'_, Db>) -> AppResult<PullRequestList> {
    pull_requests::list(&db.lock())
}

/// Cached pull requests mentioning each Jira issue, in any state.
#[tauri::command]
pub async fn list_jira_issue_pull_requests(
    db: State<'_, Db>,
) -> AppResult<Vec<JiraIssuePullRequest>> {
    pull_requests::list_for_jira_issues(&db.lock())
}

#[tauri::command]
pub async fn sync_pull_requests(app: AppHandle, db: State<'_, Db>) -> AppResult<GitHubSyncSummary> {
    let dir = app.path().app_config_dir()?;
    let config = github::load_config(&dir)?;
    let client = GitHubClient::new(github::token(&TOKEN_STORE)?)?;
    pull_requests::sync(&db, &client, &config.repositories, now_ms()).await
}

/// Searches the pull requests related to the cached Jira issues; run after a Jira sync.
/// Does nothing without a GitHub token. `all` searches done issues again too.
pub(super) async fn sync_jira_mentions(app: &AppHandle, db: &Db, all: bool) -> AppResult<()> {
    let Some(token) = TOKEN_STORE.get()? else {
        return Ok(());
    };
    let config = github::load_config(&app.path().app_config_dir()?)?;
    let client = GitHubClient::new(token)?;
    pull_requests::sync_mentions(db, &client, &config.repositories, all).await
}

/// Re-fetches one cached pull request from GitHub and returns it updated.
#[tauri::command]
pub async fn refresh_pull_request(db: State<'_, Db>, id: String) -> AppResult<PullRequest> {
    let client = GitHubClient::new(github::token(&TOKEN_STORE)?)?;
    pull_requests::refresh(&db, &client, &id).await
}

/// Opens a cached pull request in the default browser. The URL comes from the
/// database, never from the frontend.
#[tauri::command]
pub async fn open_pull_request(app: AppHandle, db: State<'_, Db>, id: String) -> AppResult<()> {
    let url = pull_requests::url(&db.lock(), &id)?
        .filter(|url| url.starts_with("https://"))
        .ok_or_else(|| AppError::Validation("This pull request is no longer available".into()))?;
    app.opener().open_url(url, None::<&str>)?;
    Ok(())
}
