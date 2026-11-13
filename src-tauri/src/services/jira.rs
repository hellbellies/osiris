use std::fs;
use std::path::Path;

use url::Url;

use crate::error::{AppError, AppResult};
use crate::models::jira::{JiraConfig, JiraSettings, JiraSettingsInput, JiraStatusRef, JiraUser};
use crate::services::jira_client::{JiraClient, JiraCredentials};
use crate::services::secrets::SecretStore;

pub const JIRA_CONFIG_FILE: &str = "jira.json";

pub fn load_config(dir: &Path) -> AppResult<JiraConfig> {
    let path = dir.join(JIRA_CONFIG_FILE);
    if !path.exists() {
        return Ok(JiraConfig::default());
    }
    let raw = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&raw)?)
}

fn save_config(dir: &Path, config: &JiraConfig) -> AppResult<()> {
    fs::create_dir_all(dir)?;
    let raw = serde_json::to_string_pretty(config)?;
    fs::write(dir.join(JIRA_CONFIG_FILE), raw)?;
    Ok(())
}

/// Validates and normalizes a Jira base URL to `scheme://host[:port][/path]`
/// without a trailing slash. An empty input means "not configured".
fn normalize_base_url(input: &str) -> AppResult<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    let invalid = || AppError::Validation(format!("'{trimmed}' is not a valid Jira URL"));
    let mut url = Url::parse(trimmed).map_err(|_| invalid())?;
    if !matches!(url.scheme(), "https" | "http") || url.host_str().is_none() {
        return Err(invalid());
    }
    url.set_query(None);
    url.set_fragment(None);
    Ok(url.as_str().trim_end_matches('/').to_owned())
}

/// Returns the stored connection details and whether a token is present.
pub fn get(dir: &Path, secrets: &impl SecretStore) -> AppResult<JiraSettings> {
    let config = load_config(dir)?;
    Ok(JiraSettings {
        base_url: config.base_url,
        email: config.email,
        project_keys: config.project_keys,
        watched_statuses: config.watched_statuses,
        has_token: secrets.get()?.is_some(),
    })
}

/// Saves URL and email to disk and, if provided, the API token to `secrets`.
pub fn save(
    dir: &Path,
    secrets: &impl SecretStore,
    input: JiraSettingsInput,
) -> AppResult<JiraSettings> {
    let project_keys = normalize_project_keys(&input.project_keys)?;
    let config = JiraConfig {
        base_url: normalize_base_url(&input.base_url)?,
        email: input.email.trim().to_owned(),
        watched_statuses: normalize_watched_statuses(&input.watched_statuses, &project_keys)?,
        project_keys,
    };
    let token = match input.api_token.as_deref().map(str::trim) {
        Some("") => return Err(AppError::Validation("API token cannot be empty".into())),
        other => other,
    };

    save_config(dir, &config)?;
    if let Some(token) = token {
        secrets.set(token)?;
    }
    get(dir, secrets)
}

/// Removes the API token from `secrets`, keeping URL and email.
pub fn clear_token(dir: &Path, secrets: &impl SecretStore) -> AppResult<JiraSettings> {
    secrets.delete()?;
    get(dir, secrets)
}

/// Checks the given settings against Jira without saving them.
/// A missing `api_token` falls back to the token stored in `secrets`.
pub async fn test_connection(
    secrets: &impl SecretStore,
    input: JiraSettingsInput,
) -> AppResult<JiraUser> {
    let base_url = normalize_base_url(&input.base_url)?;
    let token = match input.api_token.as_deref().map(str::trim) {
        Some(token) if !token.is_empty() => Some(token.to_owned()),
        _ => secrets.get()?,
    };
    let credentials = require_credentials(base_url, input.email.trim().to_owned(), token)?;
    JiraClient::new(credentials)?.myself().await
}

/// Loads the saved credentials, failing with a hint if anything is missing.
pub fn credentials(dir: &Path, secrets: &impl SecretStore) -> AppResult<JiraCredentials> {
    let config = load_config(dir)?;
    require_credentials(config.base_url, config.email, secrets.get()?)
}

fn require_credentials(
    base_url: String,
    email: String,
    token: Option<String>,
) -> AppResult<JiraCredentials> {
    if base_url.is_empty() {
        return Err(AppError::Validation("Enter the Jira site URL".into()));
    }
    if email.is_empty() {
        return Err(AppError::Validation(
            "Enter the email of your Atlassian account".into(),
        ));
    }
    let token = token.ok_or_else(|| AppError::Validation("Enter an API token".into()))?;
    Ok(JiraCredentials {
        base_url,
        email,
        token,
    })
}

/// Browser URL of an issue on the configured site. Validates `key` so only
/// `{base_url}/browse/PROJ-123` style URLs can ever be opened.
pub fn issue_url(dir: &Path, key: &str) -> AppResult<String> {
    let invalid = || AppError::Validation(format!("'{key}' is not a Jira issue key"));
    let (project, number) = key.split_once('-').ok_or_else(invalid)?;
    if !is_project_key(project) || number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(invalid());
    }
    let base_url = load_config(dir)?.base_url;
    if base_url.is_empty() {
        return Err(AppError::Validation("Enter the Jira site URL".into()));
    }
    Ok(format!("{base_url}/browse/{key}"))
}

/// Uppercases, trims and de-duplicates project keys, rejecting malformed ones.
/// Validation also keeps the keys safe to embed in JQL.
fn normalize_project_keys(keys: &[String]) -> AppResult<Vec<String>> {
    let mut normalized: Vec<String> = Vec::new();
    for key in keys.iter().map(|k| k.trim().to_ascii_uppercase()) {
        if key.is_empty() || normalized.contains(&key) {
            continue;
        }
        if !is_project_key(&key) {
            return Err(AppError::Validation(format!(
                "'{key}' is not a valid Jira project key"
            )));
        }
        normalized.push(key);
    }
    Ok(normalized)
}

/// De-duplicates watched statuses and drops those of projects that are no longer
/// configured. Status ids are numeric, which keeps them safe to embed in JQL.
fn normalize_watched_statuses(
    statuses: &[JiraStatusRef],
    project_keys: &[String],
) -> AppResult<Vec<JiraStatusRef>> {
    let mut normalized: Vec<JiraStatusRef> = Vec::new();
    for status in statuses {
        if !project_keys.contains(&status.project_key) || normalized.contains(status) {
            continue;
        }
        if status.status_id.is_empty() || !status.status_id.bytes().all(|b| b.is_ascii_digit()) {
            return Err(AppError::Validation(format!(
                "'{}' is not a Jira status id",
                status.status_id
            )));
        }
        normalized.push(status.clone());
    }
    Ok(normalized)
}

/// Jira project keys: an uppercase letter followed by uppercase letters, digits or `_`.
pub fn is_project_key(key: &str) -> bool {
    let mut chars = key.chars();
    chars.next().is_some_and(|c| c.is_ascii_uppercase())
        && chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::secrets::tests::MemoryStore;

    fn input(base_url: &str, api_token: Option<&str>) -> JiraSettingsInput {
        JiraSettingsInput {
            base_url: base_url.into(),
            email: " me@example.com ".into(),
            project_keys: Vec::new(),
            watched_statuses: Vec::new(),
            api_token: api_token.map(Into::into),
        }
    }

    fn status(project_key: &str, status_id: &str) -> JiraStatusRef {
        JiraStatusRef {
            project_key: project_key.into(),
            status_id: status_id.into(),
        }
    }

    #[test]
    fn watched_statuses_are_deduplicated_and_kept_to_configured_projects() {
        let dir = tempfile::tempdir().unwrap();
        let secrets = MemoryStore::default();
        let mut settings = input("https://x.atlassian.net", Some("t"));
        settings.project_keys = vec!["sup".into()];
        settings.watched_statuses = vec![
            status("SUP", "10646"),
            status("SUP", "10646"),
            status("DEV", "1"),
        ];
        let saved = save(dir.path(), &secrets, settings.clone()).unwrap();
        assert_eq!(saved.watched_statuses, [status("SUP", "10646")]);

        settings.watched_statuses = vec![status("SUP", "1 OR 1=1")];
        assert!(save(dir.path(), &secrets, settings).is_err());
    }

    #[test]
    fn get_returns_empty_when_unconfigured() {
        let dir = tempfile::tempdir().unwrap();
        let settings = get(dir.path(), &MemoryStore::default()).unwrap();
        assert_eq!(settings.base_url, "");
        assert!(!settings.has_token);
    }

    #[test]
    fn save_persists_config_and_token_separately() {
        let dir = tempfile::tempdir().unwrap();
        let secrets = MemoryStore::default();
        let settings = save(
            dir.path(),
            &secrets,
            input("https://acme.atlassian.net/", Some(" secret ")),
        )
        .unwrap();

        assert_eq!(settings.base_url, "https://acme.atlassian.net");
        assert_eq!(settings.email, "me@example.com");
        assert!(settings.has_token);
        assert_eq!(secrets.get().unwrap().as_deref(), Some("secret"));

        let on_disk = fs::read_to_string(dir.path().join(JIRA_CONFIG_FILE)).unwrap();
        assert!(!on_disk.contains("secret"));
    }

    #[test]
    fn save_without_token_keeps_existing_token() {
        let dir = tempfile::tempdir().unwrap();
        let secrets = MemoryStore::default();
        save(
            dir.path(),
            &secrets,
            input("https://a.atlassian.net", Some("t1")),
        )
        .unwrap();
        save(dir.path(), &secrets, input("https://b.atlassian.net", None)).unwrap();
        assert_eq!(secrets.get().unwrap().as_deref(), Some("t1"));
    }

    #[test]
    fn save_rejects_invalid_url_and_blank_token() {
        let dir = tempfile::tempdir().unwrap();
        let secrets = MemoryStore::default();
        for bad in ["acme.atlassian.net", "ftp://acme.net", "https://"] {
            assert!(matches!(
                save(dir.path(), &secrets, input(bad, None)),
                Err(AppError::Validation(_))
            ));
        }
        assert!(matches!(
            save(
                dir.path(),
                &secrets,
                input("https://a.atlassian.net", Some("  "))
            ),
            Err(AppError::Validation(_))
        ));
        assert!(!dir.path().join(JIRA_CONFIG_FILE).exists());
    }

    #[test]
    fn normalize_keeps_context_path_and_drops_query() {
        assert_eq!(
            normalize_base_url("https://jira.corp.local:8443/jira/?x=1#y").unwrap(),
            "https://jira.corp.local:8443/jira"
        );
    }

    #[test]
    fn clear_token_keeps_config() {
        let dir = tempfile::tempdir().unwrap();
        let secrets = MemoryStore::default();
        save(
            dir.path(),
            &secrets,
            input("https://a.atlassian.net", Some("t")),
        )
        .unwrap();
        let settings = clear_token(dir.path(), &secrets).unwrap();
        assert!(!settings.has_token);
        assert_eq!(settings.base_url, "https://a.atlassian.net");
    }

    #[tokio::test]
    async fn test_connection_falls_back_to_stored_token() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/rest/api/2/myself")
            .match_header("authorization", "Basic bWVAZXhhbXBsZS5jb206c2VjcmV0")
            .with_body(r#"{"displayName":"Ada"}"#)
            .create_async()
            .await;
        let secrets = MemoryStore::default();
        secrets.set("secret").unwrap();

        let user = test_connection(&secrets, input(&server.url(), None))
            .await
            .unwrap();
        assert_eq!(user.display_name, "Ada");
        assert_eq!(user.email_address, None);
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_connection_requires_all_fields() {
        let secrets = MemoryStore::default();
        for bad in [
            input("", Some("secret")),
            JiraSettingsInput {
                email: " ".into(),
                ..input("https://a.atlassian.net", Some("secret"))
            },
            input("https://a.atlassian.net", None),
        ] {
            assert!(matches!(
                test_connection(&secrets, bad).await,
                Err(AppError::Validation(_))
            ));
        }
    }

    #[test]
    fn save_normalizes_project_keys() {
        let dir = tempfile::tempdir().unwrap();
        let secrets = MemoryStore::default();
        let settings = save(
            dir.path(),
            &secrets,
            JiraSettingsInput {
                project_keys: vec![" foo".into(), "".into(), "BAR_2".into(), "FOO".into()],
                ..input("https://a.atlassian.net", None)
            },
        )
        .unwrap();
        assert_eq!(settings.project_keys, ["FOO", "BAR_2"]);
    }

    #[test]
    fn save_rejects_malformed_project_keys() {
        let dir = tempfile::tempdir().unwrap();
        for bad in ["2FOO", "FOO-1", "FO O", "FOO\")"] {
            let result = save(
                dir.path(),
                &MemoryStore::default(),
                JiraSettingsInput {
                    project_keys: vec![bad.into()],
                    ..input("https://a.atlassian.net", None)
                },
            );
            assert!(matches!(result, Err(AppError::Validation(_))), "{bad}");
        }
    }

    #[test]
    fn issue_url_validates_key() {
        let dir = tempfile::tempdir().unwrap();
        save(
            dir.path(),
            &MemoryStore::default(),
            input("https://a.atlassian.net/", None),
        )
        .unwrap();
        assert_eq!(
            issue_url(dir.path(), "FOO-123").unwrap(),
            "https://a.atlassian.net/browse/FOO-123"
        );
        for bad in ["FOO", "FOO-", "FOO-12a", "foo-1", "../x-1", "FOO-1?x=y"] {
            assert!(issue_url(dir.path(), bad).is_err(), "{bad}");
        }
    }
}
