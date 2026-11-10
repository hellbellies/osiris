use serde::{Serialize, Serializer, ser::SerializeStruct};

/// Application-wide error type returned from every IPC command.
///
/// Serialized to the frontend as `{ kind, message }` — mirrored by
/// `AppError` in `src/types/error.ts`.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("tauri error: {0}")]
    Tauri(#[from] tauri::Error),

    #[error("could not open link: {0}")]
    Opener(#[from] tauri_plugin_opener::Error),

    #[error("keychain error: {0}")]
    Keyring(#[from] keyring::Error),

    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("database migration error: {0}")]
    Migration(#[from] rusqlite_migration::Error),

    #[error("{0}")]
    Validation(String),

    #[error("{0}")]
    Jira(String),

    #[error("{0}")]
    GitHub(String),

    #[error("{0}")]
    Obsidian(String),
}

impl AppError {
    fn kind(&self) -> &'static str {
        match self {
            AppError::Io(_) => "io",
            AppError::Serde(_) => "serde",
            AppError::Tauri(_) | AppError::Opener(_) => "tauri",
            AppError::Keyring(_) => "keyring",
            AppError::Db(_) | AppError::Migration(_) => "db",
            AppError::Validation(_) => "validation",
            AppError::Jira(_) => "jira",
            AppError::GitHub(_) => "github",
            AppError::Obsidian(_) => "obsidian",
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct("AppError", 2)?;
        state.serialize_field("kind", self.kind())?;
        state.serialize_field("message", &self.to_string())?;
        state.end()
    }
}

pub type AppResult<T> = Result<T, AppError>;
