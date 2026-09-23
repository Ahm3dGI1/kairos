use std::path::PathBuf;
use std::sync::Mutex;

use kairos_core::vault::Vault;
use kairos_core::{Settings, Store};
use tauri::{AppHandle, Emitter, Manager};

pub type Fingerprint = std::collections::BTreeMap<PathBuf, Vec<u8>>;

pub struct AppState {
    pub io: Mutex<()>,
    pub fingerprint: Mutex<Fingerprint>,
    pub store: Mutex<Store>,
    /// Absent when the user has turned the vault off.
    pub vault: Mutex<Option<Vault>>,
    pub settings: Mutex<Settings>,
    pub paths: Paths,
}

#[derive(Clone)]
pub struct Paths {
    /// Where `tasks.db` and the bootstrap pointer live.
    pub data_dir: PathBuf,
    /// The vault folder, whether or not the vault is enabled.
    pub vault_dir: PathBuf,
}

impl Paths {
    pub fn settings_file(&self) -> PathBuf {
        self.vault_dir.join("settings.json")
    }
}

/// Event every window listens for: stored data changed, re-read your view.
/// Covers tasks, habits and the journal alike — a window knows which parts it
/// draws, and re-reading the wrong one costs a local query.
pub const DATA_CHANGED: &str = "data-changed";

/// Remembers where the vault is, so the settings inside it can be found again.
const POINTER: &str = "vault-path.txt";

/// Opens the vault and the database, and brings them into agreement.
pub fn init(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = app.path().app_data_dir()?;
    std::fs::create_dir_all(&data_dir)?;

    let home = app.path().home_dir().ok();
    let default_vault = match &home {
        Some(home) => {
            let named = home.join("Kairos");
            // The app was called Master Todo before, and the rename must not
            // orphan a vault someone is already keeping notes in. The old
            // folder is adopted as-is rather than moved: it may be a git
            // checkout or a synced folder, and moving it would break more than
            // it tidied.
            let previous = home.join("Master Todo");
            if !named.exists() && previous.exists() {
                previous
            } else {
                named
            }
        }
        None => data_dir.join("vault"),
    };

    // The pointer wins if it is there; otherwise the default, which the first
    // run writes down.
    let vault_dir = std::fs::read_to_string(data_dir.join(POINTER))
        .ok()
        .map(|text| PathBuf::from(text.trim()))
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(default_vault);

    let paths = Paths { data_dir: data_dir.clone(), vault_dir: vault_dir.clone() };
    let mut settings = Settings::load(paths.settings_file());

    // A vault path typed into settings.json moves the vault.
    let vault_dir = if settings.vault_path.trim().is_empty() {
        vault_dir
    } else {
        PathBuf::from(settings.vault_path.trim())
    };
    let paths = Paths { data_dir, vault_dir: vault_dir.clone() };
    if settings.vault_path.trim().is_empty() {
        settings.vault_path = vault_dir.to_string_lossy().to_string();
    }

    let mut store = Store::open(paths.data_dir.join("tasks.db"))?;
    let vault = settings.vault_enabled.then(|| Vault::new(vault_dir.clone()));

    if let Some(vault) = &vault {
        let today = chrono::Local::now().date_naive();
        if vault.exists() {
            // The files are the record, so they win — including over whatever
            // the database thought it knew. That is a destructive step, and the
            // vault can be somewhere that is only half there: a cloud folder
            // mid-sync, a network share that has not come back. So take a copy
            // of what is about to be replaced first.
            back_up(&paths.data_dir, &store);

            // Never fatal. The vault is a folder of text files that anyone can
            // edit, so it can say something the database cannot accept — and
            // an app that refuses to start because of it is an app you cannot
            // use to fix it. Carry on with the database as it stands and say
            // what went wrong.
            match vault
                .read(today)
                .map_err(|e| e.to_string())
                .and_then(|snapshot| store.restore_external(&snapshot).map_err(|e| e.to_string()))
            {
                Ok(()) => {}
                Err(error) => eprintln!("could not read the vault, keeping the database: {error}"),
            }
        } else {
            // First run, or a vault the user moved to an empty folder: seed it
            // from whatever the database already holds.
            if let Err(error) = store
                .snapshot()
                .map_err(|e| e.to_string())
                .and_then(|snapshot| vault.write(&snapshot).map_err(|e| e.to_string()))
            {
                eprintln!("could not seed the vault: {error}");
            }
        }
    }

    let _ = std::fs::write(paths.data_dir.join(POINTER), vault_dir.to_string_lossy().as_ref());
    if !paths.settings_file().exists() {
        settings.save(paths.settings_file())?;
    }

    let fingerprint = vault.as_ref().and_then(|v| v.fingerprint().ok()).unwrap_or_default();
    app.manage(AppState {
        io: Mutex::new(()),
        fingerprint: Mutex::new(fingerprint),
        store: Mutex::new(store),
        vault: Mutex::new(vault),
        settings: Mutex::new(settings),
        paths,
    });
    Ok(())
}

/// How many startup backups to keep. Enough to cover a problem noticed a few
/// launches later, few enough that they never amount to anything.
const BACKUPS: usize = 5;

/// Writes the database out as one JSON file before an import replaces it.
///
/// Never fatal: a backup that cannot be written is a reason to log, not a
/// reason to refuse to start.
fn back_up(data_dir: &std::path::Path, store: &Store) {
    let Ok(snapshot) = store.snapshot() else { return };
    if snapshot.is_empty() {
        return;
    }

    let dir = data_dir.join("backups");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let name = format!("{}.json", chrono::Local::now().format("%Y-%m-%d-%H%M%S"));
    match serde_json::to_string(&snapshot) {
        Ok(text) => {
            if let Err(error) = std::fs::write(dir.join(name), text) {
                eprintln!("could not write a backup: {error}");
                return;
            }
        }
        Err(error) => {
            eprintln!("could not serialise a backup: {error}");
            return;
        }
    }

    let Ok(entries) = std::fs::read_dir(&dir) else { return };
    let mut files: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    files.sort();
    for old in files.iter().rev().skip(BACKUPS) {
        let _ = std::fs::remove_file(old);
    }
}

/// Mirrors the database back out to the files.
pub fn export(state: &AppState) -> Result<(), String> {
    let _guard = state.io.lock().map_err(|_| "storage lock poisoned")?;
    sync_files(state)?;
    let store = state.store.lock().map_err(|_| "store lock poisoned")?;
    export_store(state, &store)
}

pub fn export_store(state: &AppState, store: &Store) -> Result<(), String> {
    let vault = state.vault.lock().map_err(|_| "vault lock poisoned")?;
    if let Some(vault) = vault.as_ref() {
        let mut last = state.fingerprint.lock().map_err(|_| "fingerprint lock poisoned")?;
        if vault.fingerprint().map_err(|e| e.to_string())? != *last {
            return Err(
                "Files changed during this edit. Your edit was not saved; reload and try again."
                    .into(),
            );
        }
        let snapshot = store.snapshot().map_err(|e| e.to_string())?;
        *last = vault
            .write_checked(&snapshot, &last)
            .map_err(|e| format!("Could not save the vault: {e}"))?;
    }
    Ok(())
}

// Caller holds io throughout import, mutation and export.
pub fn sync_files(state: &AppState) -> Result<bool, String> {
    let vault = state.vault.lock().map_err(|_| "vault lock poisoned")?;
    let Some(vault) = vault.as_ref() else { return Ok(false) };
    let current = vault.fingerprint().map_err(|e| e.to_string())?;
    let mut last = state.fingerprint.lock().map_err(|_| "fingerprint lock poisoned")?;
    if current == *last {
        return Ok(false);
    }
    for dir in ["tasks", "habits", "workouts"] {
        if last.keys().any(|p| p.starts_with(dir)) && !vault.root().join(dir).is_dir() {
            return Err(format!("The {dir} folder is unavailable. Restore it before importing."));
        }
    }
    let snapshot = vault.read(chrono::Local::now().date_naive()).map_err(|e| e.to_string())?;
    if vault.fingerprint().map_err(|e| e.to_string())? != current {
        return Err("Vault is still changing; retry after the editor finishes saving.".into());
    }
    let mut store = state.store.lock().map_err(|_| "store lock poisoned")?;
    back_up(&state.paths.data_dir, &store);
    store.restore_external(&snapshot).map_err(|e| e.to_string())?;
    *last = current;
    Ok(true)
}

pub fn import(state: &AppState) -> Result<bool, String> {
    let _guard = state.io.lock().map_err(|_| "storage lock poisoned")?;
    sync_files(state)
}

/// Tells every window the task list moved under it, and refreshes the tray
/// tooltip so a hover is never stale.
pub fn notify_changed(app: &AppHandle) {
    let _ = app.emit(DATA_CHANGED, ());
    crate::reminders::update_tooltip(app);
}

#[cfg(test)]
mod tests {
    use super::*;
    use kairos_core::Task;
    struct TestState {
        root: PathBuf,
        state: AppState,
    }
    impl TestState {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("kairos-shell-test-{}", uuid::Uuid::new_v4()));
            let vault = Vault::new(root.join("vault"));
            let store = Store::in_memory().unwrap();
            vault.write(&store.snapshot().unwrap()).unwrap();
            let fingerprint = vault.fingerprint().unwrap();
            let state = AppState {
                io: Mutex::new(()),
                fingerprint: Mutex::new(fingerprint),
                store: Mutex::new(store),
                vault: Mutex::new(Some(vault)),
                settings: Mutex::new(Settings::default()),
                paths: Paths { data_dir: root.join("data"), vault_dir: root.join("vault") },
            };
            Self { root, state }
        }
        fn file(&self) -> PathBuf {
            self.state.paths.vault_dir.join("tasks/inbox.md")
        }
    }
    impl Drop for TestState {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn imports_external_edits_before_the_next_app_write() {
        let test = TestState::new();
        std::fs::write(test.file(), "# Inbox\n- [ ] External task\n").unwrap();
        assert!(sync_files(&test.state).unwrap());
        let mut store = test.state.store.lock().unwrap();
        store.save(&Task::new("App task", chrono::Local::now().date_naive())).unwrap();
        export_store(&test.state, &store).unwrap();
        let text = std::fs::read_to_string(test.file()).unwrap();
        assert!(text.contains("External task") && text.contains("App task"));
    }

    #[test]
    fn refuses_to_overwrite_a_file_changed_after_import() {
        let test = TestState::new();
        let store = test.state.store.lock().unwrap();
        std::fs::write(test.file(), "# Inbox\n- [ ] Do not overwrite\n").unwrap();
        assert!(export_store(&test.state, &store).is_err());
        assert!(std::fs::read_to_string(test.file()).unwrap().contains("Do not overwrite"));
    }

    #[test]
    fn missing_directory_does_not_empty_the_database() {
        let test = TestState::new();
        std::fs::remove_file(test.file()).unwrap();
        std::fs::remove_dir(test.state.paths.vault_dir.join("tasks")).unwrap();
        assert!(sync_files(&test.state).is_err());
    }

    #[test]
    fn fingerprint_detects_renames_and_content_changes() {
        let test = TestState::new();
        let vault = test.state.vault.lock().unwrap();
        let vault = vault.as_ref().unwrap();
        let before = vault.fingerprint().unwrap();
        std::fs::rename(test.file(), test.file().with_file_name("renamed.md")).unwrap();
        assert_ne!(vault.fingerprint().unwrap(), before);
    }
}
