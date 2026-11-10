use serde::Serialize;

/// How far Obsidian is set up for osiris, checked step by step.
/// Mirrored by `ObsidianState` in `src/types/obsidian.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ObsidianState {
    /// No Obsidian app in `/Applications`.
    NotInstalled,
    /// Older than the first version with the command line interface.
    Outdated,
    /// Installed, but not running: the CLI would launch it, so osiris doesn't call it.
    NotRunning,
    /// Running, but the command line interface is turned off in its settings.
    CliDisabled,
    /// Everything works.
    Ready,
    /// The CLI answered something unexpected; see `ObsidianStatus::message`.
    Failed,
}

/// A vault Obsidian knows about. Mirrored by `ObsidianVault` in `src/types/obsidian.ts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObsidianVault {
    /// The vault folder's name, which the CLI and `obsidian://` links use.
    pub name: String,
    pub path: String,
    /// Open in an Obsidian window. Only open vaults are searched: the CLI opens
    /// any other vault it touches.
    pub open: bool,
}

/// Mirrored by `ObsidianStatus` in `src/types/obsidian.ts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObsidianStatus {
    pub state: ObsidianState,
    /// The installed app's version, if installed.
    pub version: Option<String>,
    pub vaults: Vec<ObsidianVault>,
    /// Details for `Failed`.
    pub message: Option<String>,
}

/// A note in a vault. Mirrored by `ObsidianNote` in `src/types/obsidian.ts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObsidianNote {
    /// `<vault>/<path>`; the item id in workstreams.
    pub id: String,
    pub vault: String,
    /// Path inside the vault, e.g. `Meetings/Planning.md`.
    pub path: String,
    /// File name without `.md`, as Obsidian shows it.
    pub title: String,
    /// Folder inside the vault; empty at the vault's top level.
    pub folder: String,
    pub workstream_id: Option<i64>,
}
