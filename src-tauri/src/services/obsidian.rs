//! Obsidian notes as workstream items, through Obsidian's command line interface.
//!
//! The CLI (`obsidian-cli` inside the app, Obsidian 1.12.7+) talks to the running
//! app, and launches Obsidian when it isn't running; it also opens any vault it
//! touches. So osiris calls it only while Obsidian runs, and searches only vaults
//! that are already open. It always exits with 0: failures are recognized by text.
//!
//! Linked notes are cached in `obsidian_notes` (vault and path only), so workstreams
//! show them without Obsidian. Unlinked notes are pruned; the files are never touched.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use rusqlite::{Connection, params};
use serde_json::Value;

use crate::error::{AppError, AppResult};
use crate::models::obsidian::{ObsidianNote, ObsidianState, ObsidianStatus, ObsidianVault};

/// First version with the command line interface.
const MIN_VERSION: [u32; 3] = [1, 12, 7];
const APP_BUNDLE: &str = "/Applications/Obsidian.app";
/// A CLI call normally takes well under a second.
const CLI_TIMEOUT: Duration = Duration::from_secs(10);
const CLI_DISABLED: &str = "Command line interface is not enabled";

/// What osiris needs from the Obsidian installation; faked in tests.
pub trait Obsidian {
    /// The installed app's version, `None` if not installed.
    fn version(&self) -> Option<String>;
    fn is_running(&self) -> bool;
    /// The vaults Obsidian knows about, from its own settings file.
    fn vaults(&self) -> AppResult<Vec<ObsidianVault>>;
    /// Runs the CLI with `args` and returns its trimmed output.
    fn run(&self, args: &[String]) -> AppResult<String>;
}

/// The real Obsidian app on macOS.
pub struct ObsidianApp {
    bundle: PathBuf,
    /// Obsidian's `obsidian.json`, which lists the vaults.
    config_file: PathBuf,
}

impl ObsidianApp {
    /// `config_dir` is the user's configuration directory (`~/Library/Application Support`).
    pub fn new(config_dir: &Path) -> Self {
        Self {
            bundle: PathBuf::from(APP_BUNDLE),
            config_file: config_dir.join("obsidian").join("obsidian.json"),
        }
    }
}

impl Obsidian for ObsidianApp {
    fn version(&self) -> Option<String> {
        let plist = std::fs::read_to_string(self.bundle.join("Contents/Info.plist")).ok()?;
        plist_string(&plist, "CFBundleShortVersionString")
    }

    fn is_running(&self) -> bool {
        Command::new("pgrep")
            .args(["-x", "Obsidian"])
            .output()
            .is_ok_and(|output| output.status.success())
    }

    fn vaults(&self) -> AppResult<Vec<ObsidianVault>> {
        parse_vaults(&std::fs::read_to_string(&self.config_file)?)
    }

    fn run(&self, args: &[String]) -> AppResult<String> {
        let cli = self.bundle.join("Contents/MacOS/obsidian-cli");
        let mut child = Command::new(cli)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| AppError::Obsidian(format!("Could not run Obsidian's CLI: {err}")))?;
        // Read while waiting, so a large result can't fill the pipe and stall the CLI.
        let mut stdout = child.stdout.take().expect("stdout is piped");
        let reader = std::thread::spawn(move || {
            let mut text = String::new();
            stdout.read_to_string(&mut text).map(|_| text)
        });
        let deadline = Instant::now() + CLI_TIMEOUT;
        while child.try_wait()?.is_none() {
            if Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AppError::Obsidian("Obsidian did not respond".into()));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let mut text = reader
            .join()
            .map_err(|_| AppError::Obsidian("Could not read Obsidian's answer".into()))??;
        if let Some(mut stderr) = child.stderr.take() {
            stderr.read_to_string(&mut text)?;
        }
        Ok(text.trim().to_owned())
    }
}

/// Checks each prerequisite in turn; the first one missing decides the state.
pub fn status(obsidian: &impl Obsidian) -> ObsidianStatus {
    let version = obsidian.version();
    let vaults = obsidian.vaults().unwrap_or_default();
    let (state, message) = match &version {
        None => (ObsidianState::NotInstalled, None),
        Some(v) if !is_supported(v) => (ObsidianState::Outdated, None),
        Some(_) if !obsidian.is_running() => (ObsidianState::NotRunning, None),
        Some(_) => match obsidian.run(&["version".into()]).and_then(checked) {
            Ok(_) => (ObsidianState::Ready, None),
            Err(AppError::Validation(_)) => (ObsidianState::CliDisabled, None),
            Err(err) => (ObsidianState::Failed, Some(err.to_string())),
        },
    };
    ObsidianStatus {
        state,
        version,
        vaults,
        message,
    }
}

/// Notes in the open vaults whose name or text matches `term` (Obsidian's search
/// syntax), at most `limit` per vault, as `(vault, path)`. Empty if Obsidian isn't
/// installed; an error if it isn't running or its CLI is off.
pub fn search(
    obsidian: &impl Obsidian,
    term: &str,
    limit: usize,
) -> AppResult<Vec<(String, String)>> {
    let term = term.trim();
    if term.is_empty() || obsidian.version().is_none() {
        return Ok(Vec::new());
    }
    if !obsidian.is_running() {
        return Err(AppError::Obsidian(
            "Start Obsidian to search its notes".into(),
        ));
    }
    let mut found = Vec::new();
    for vault in obsidian.vaults()?.into_iter().filter(|vault| vault.open) {
        let output = obsidian.run(&[
            format!("vault={}", vault.name),
            "search".into(),
            format!("query={term}"),
            format!("limit={limit}"),
            "format=json".into(),
        ])?;
        found.extend(
            parse_search(&output)?
                .into_iter()
                .filter(|path| is_note_path(path))
                .map(|path| (vault.name.clone(), path)),
        );
    }
    Ok(found)
}

/// The CLI's output, or why it failed. A disabled CLI is a `Validation` error.
fn checked(output: String) -> AppResult<String> {
    if output.starts_with(CLI_DISABLED) {
        return Err(AppError::Validation(
            "Turn on Obsidian's command line interface: Settings → General → Advanced".into(),
        ));
    }
    if output.starts_with("Error:") || output == "Vault not found." {
        return Err(AppError::Obsidian(format!("Obsidian: {output}")));
    }
    Ok(output)
}

/// Paths from `search … format=json`, which prints plain text when nothing matches.
fn parse_search(output: &str) -> AppResult<Vec<String>> {
    let output = checked(output.to_owned())?;
    if output.starts_with('[') {
        return serde_json::from_str(&output)
            .map_err(|err| AppError::Obsidian(format!("Unexpected search result: {err}")));
    }
    if output.is_empty() || output == "No matches found." {
        return Ok(Vec::new());
    }
    Err(AppError::Obsidian(format!("Obsidian: {output}")))
}

/// The `<string>` following `<key>key</key>` in an XML property list.
fn plist_string(plist: &str, key: &str) -> Option<String> {
    let rest = &plist[plist.find(&format!("<key>{key}</key>"))?..];
    let start = rest.find("<string>")? + "<string>".len();
    let end = rest[start..].find("</string>")? + start;
    Some(rest[start..end].trim().to_owned())
}

fn is_supported(version: &str) -> bool {
    let mut parts = version
        .split('.')
        .map(|part| part.parse::<u32>().unwrap_or(0));
    let parsed = [(); 3].map(|_| parts.next().unwrap_or(0));
    parsed >= MIN_VERSION
}

/// Vaults from Obsidian's `obsidian.json`, sorted by name.
fn parse_vaults(json: &str) -> AppResult<Vec<ObsidianVault>> {
    let config: Value = serde_json::from_str(json)?;
    let mut vaults: Vec<ObsidianVault> = (config["vaults"].as_object().into_iter())
        .flat_map(|vaults| vaults.values())
        .filter_map(|vault| {
            let path = vault["path"].as_str()?;
            let name = Path::new(path).file_name()?.to_str()?.to_owned();
            Some(ObsidianVault {
                name,
                path: path.to_owned(),
                open: vault["open"].as_bool().unwrap_or(false),
            })
        })
        .collect();
    vaults.sort_by_key(|vault| vault.name.to_lowercase());
    Ok(vaults)
}

/// A Markdown file inside a vault: relative, without `..` components.
fn is_note_path(path: &str) -> bool {
    path.ends_with(".md")
        && !path.starts_with('/')
        && !path.split('/').any(|part| part.is_empty() || part == "..")
}

/// `(vault, path)` of a note id (`<vault>/<path>`), if it is one.
fn parse_id(id: &str) -> Option<(&str, &str)> {
    let (vault, path) = id.split_once('/')?;
    (!vault.is_empty() && vault != ".." && is_note_path(path)).then_some((vault, path))
}

fn note(vault: &str, path: &str, workstream_id: Option<i64>) -> ObsidianNote {
    let (folder, file) = path.rsplit_once('/').unwrap_or(("", path));
    ObsidianNote {
        id: format!("{vault}/{path}"),
        vault: vault.to_owned(),
        path: path.to_owned(),
        title: file.strip_suffix(".md").unwrap_or(file).to_owned(),
        folder: folder.to_owned(),
        workstream_id,
    }
}

/// Search results as notes, with the workstream each one is in (if any).
pub fn with_workstreams(
    conn: &Connection,
    found: &[(String, String)],
) -> AppResult<Vec<ObsidianNote>> {
    let mut lookup = conn.prepare(
        "SELECT workstream_id FROM workstream_items
         WHERE item_type = 'obsidian_note' AND item_id = ?1",
    )?;
    found
        .iter()
        .map(|(vault, path)| {
            let id = format!("{vault}/{path}");
            let workstream_id = lookup
                .query_map([&id], |row| row.get(0))?
                .next()
                .transpose()?;
            Ok(note(vault, path, workstream_id))
        })
        .collect()
}

/// Caches a note so it can be linked; `false` if `id` is not a note id.
/// The file itself isn't checked: osiris doesn't read vaults.
pub fn register(conn: &Connection, id: &str) -> AppResult<bool> {
    let Some((vault, path)) = parse_id(id) else {
        return Ok(false);
    };
    conn.execute(
        "INSERT OR IGNORE INTO obsidian_notes (id, vault, path) VALUES (?1, ?2, ?3)",
        params![id, vault, path],
    )?;
    Ok(true)
}

/// Forgets notes no longer linked to a workstream.
pub fn prune_unlinked(conn: &Connection) -> AppResult<usize> {
    Ok(conn.execute(
        "DELETE FROM obsidian_notes WHERE id NOT IN
             (SELECT item_id FROM workstream_items WHERE item_type = 'obsidian_note')",
        [],
    )?)
}

/// Notes linked to a workstream, by folder and title.
pub fn list_for_workstream(conn: &Connection, workstream_id: i64) -> AppResult<Vec<ObsidianNote>> {
    let mut notes = conn
        .prepare(
            "SELECT n.vault, n.path FROM obsidian_notes n
             JOIN workstream_items w ON w.item_type = 'obsidian_note' AND w.item_id = n.id
             WHERE w.workstream_id = ?1",
        )?
        .query_map([workstream_id], |row| {
            Ok(note(
                &row.get::<_, String>(0)?,
                &row.get::<_, String>(1)?,
                Some(workstream_id),
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    notes.sort_by(|a, b| (&a.vault, a.path.to_lowercase()).cmp(&(&b.vault, b.path.to_lowercase())));
    Ok(notes)
}

/// The `obsidian://` link that opens a note, which works even with the CLI off.
pub fn open_url(id: &str) -> AppResult<url::Url> {
    let (vault, path) =
        parse_id(id).ok_or_else(|| AppError::Validation("Not an Obsidian note".into()))?;
    let link = format!(
        "obsidian://open?vault={}&file={}",
        encode(vault),
        encode(path)
    );
    url::Url::parse(&link).map_err(|err| AppError::Obsidian(format!("Invalid note link: {err}")))
}

/// Percent-encodes everything but unreserved characters, as `encodeURIComponent`
/// does (Obsidian links expect `%20`, not the `+` of form encoding).
fn encode(text: &str) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use crate::db;

    /// A scripted Obsidian: `outputs` answers CLI calls in order.
    #[derive(Default)]
    struct Fake {
        version: Option<&'static str>,
        running: bool,
        vaults: Vec<ObsidianVault>,
        outputs: RefCell<Vec<&'static str>>,
        calls: RefCell<Vec<Vec<String>>>,
    }

    impl Obsidian for Fake {
        fn version(&self) -> Option<String> {
            self.version.map(Into::into)
        }
        fn is_running(&self) -> bool {
            self.running
        }
        fn vaults(&self) -> AppResult<Vec<ObsidianVault>> {
            Ok(self.vaults.clone())
        }
        fn run(&self, args: &[String]) -> AppResult<String> {
            self.calls.borrow_mut().push(args.to_vec());
            Ok(self.outputs.borrow_mut().remove(0).into())
        }
    }

    fn vault(name: &str, open: bool) -> ObsidianVault {
        ObsidianVault {
            name: name.into(),
            path: format!("/Users/me/Obsidian/{name}"),
            open,
        }
    }

    fn ready(outputs: Vec<&'static str>) -> Fake {
        Fake {
            version: Some("1.15.0"),
            running: true,
            vaults: vec![vault("Work", true), vault("Music", false)],
            outputs: RefCell::new(outputs),
            ..Fake::default()
        }
    }

    #[test]
    fn status_reports_the_first_missing_prerequisite() {
        let state = |fake: Fake| status(&fake).state;
        assert_eq!(state(Fake::default()), ObsidianState::NotInstalled);
        let outdated = Fake {
            version: Some("1.12.6"),
            running: true,
            ..Fake::default()
        };
        assert_eq!(state(outdated), ObsidianState::Outdated);
        let stopped = Fake {
            version: Some("1.12.7"),
            ..Fake::default()
        };
        assert_eq!(state(stopped), ObsidianState::NotRunning);
        assert_eq!(
            state(ready(vec![
                "Command line interface is not enabled. Please turn it on in Settings > General > Advanced."
            ])),
            ObsidianState::CliDisabled
        );
        assert_eq!(
            state(ready(vec!["1.15.0 (installer 1.15.0)"])),
            ObsidianState::Ready
        );
        let failed = status(&ready(vec!["Error: something broke"]));
        assert_eq!(failed.state, ObsidianState::Failed);
        assert_eq!(
            failed.message.as_deref(),
            Some("Obsidian: Error: something broke")
        );
    }

    #[test]
    fn search_asks_only_open_vaults_and_keeps_markdown_notes() {
        let fake = ready(vec![
            r#"["Meetings/Plan.md","Board.canvas","../escape.md"]"#,
        ]);
        let found = search(&fake, " plan ", 20).unwrap();
        assert_eq!(
            found,
            [("Work".to_string(), "Meetings/Plan.md".to_string())]
        );
        assert_eq!(
            fake.calls.borrow()[0],
            [
                "vault=Work",
                "search",
                "query=plan",
                "limit=20",
                "format=json"
            ]
        );

        assert!(
            search(&ready(vec!["No matches found."]), "x", 20)
                .unwrap()
                .is_empty()
        );
        assert!(
            search(
                &ready(vec!["Command line interface is not enabled."]),
                "x",
                20
            )
            .is_err()
        );
        assert!(search(&ready(vec![]), "  ", 20).unwrap().is_empty());
        assert!(search(&Fake::default(), "x", 20).unwrap().is_empty());
        let stopped = Fake {
            version: Some("1.15.0"),
            ..Fake::default()
        };
        assert!(search(&stopped, "x", 20).is_err());
    }

    #[test]
    fn reads_version_and_vaults_from_obsidians_files() {
        let plist = "<dict>\n<key>CFBundleName</key>\n<string>Obsidian</string>\n\
                     <key>CFBundleShortVersionString</key>\n<string>1.15.0</string>\n</dict>";
        assert_eq!(
            plist_string(plist, "CFBundleShortVersionString").as_deref(),
            Some("1.15.0")
        );
        assert!(is_supported("1.15.0") && is_supported("1.12.7") && is_supported("2.0"));
        assert!(!is_supported("1.12.6") && !is_supported("1.9.10"));

        let vaults = parse_vaults(
            r#"{"vaults":{"a":{"path":"/Users/me/Obsidian/Work","ts":1,"open":true},
                          "b":{"path":"/Users/me/Notes/archive","ts":2}}}"#,
        )
        .unwrap();
        assert_eq!(
            vaults,
            [
                vault_at("archive", "/Users/me/Notes/archive", false),
                vault_at("Work", "/Users/me/Obsidian/Work", true)
            ]
        );
    }

    fn vault_at(name: &str, path: &str, open: bool) -> ObsidianVault {
        ObsidianVault {
            name: name.into(),
            path: path.into(),
            open,
        }
    }

    #[test]
    fn note_ids_and_links() {
        assert_eq!(
            parse_id("Work/Meetings/Plan.md"),
            Some(("Work", "Meetings/Plan.md"))
        );
        for bad in [
            "Work",
            "Work/Plan.txt",
            "/Plan.md",
            "Work//Plan.md",
            "Work/../Plan.md",
            "../x/Plan.md",
        ] {
            assert_eq!(parse_id(bad), None, "{bad}");
        }
        let note = note("Work", "Meetings/Plan A.md", None);
        assert_eq!(
            (note.title.as_str(), note.folder.as_str()),
            ("Plan A", "Meetings")
        );
        assert_eq!(
            open_url("Work/Meetings/Plan A.md").unwrap().as_str(),
            "obsidian://open?vault=Work&file=Meetings%2FPlan%20A.md"
        );
    }

    #[test]
    fn registered_notes_are_pruned_once_unlinked() {
        let conn = db::open_in_memory();
        assert!(!register(&conn, "not a note").unwrap());
        assert!(register(&conn, "Work/Plan.md").unwrap());
        assert_eq!(prune_unlinked(&conn).unwrap(), 1);
    }
}
