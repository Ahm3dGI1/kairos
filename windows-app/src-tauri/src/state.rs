//! Application state: one open store, one vault, one set of settings, shared
//! across every window.
//!
//! The tray, the widget, and the quick-add overlay are separate webviews but
//! one app — they must see the same tasks the instant either changes them, so
//! they share a single [`Store`] rather than each opening the database.
//!
//! The vault is the system of record and the database is its index, so the
//! order of operations at startup matters: read the files, rebuild the
//! database from them, and only then show anything. After that every mutation
//! writes the database and mirrors straight back out to the files.

use std::path::PathBuf;
use std::sync::Mutex;

use kairos_core::vault::Vault;
use kairos_core::{Settings, Store};
use tauri::{AppHandle, Emitter, Manager};

pub struct AppState {
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
///
/// One line of text rather than a second config format: everything else the
/// user might want to change lives in the vault, and this file exists only to
/// point at it.
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
                .and_then(|snapshot| store.restore(&snapshot).map_err(|e| e.to_string()))
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
    let _ = settings.save(paths.settings_file());

    app.manage(AppState {
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
///
/// Cheap to call after every mutation: the vault only touches a file whose
/// contents actually changed, so a completed task rewrites one file and leaves
/// the rest of the folder's timestamps alone.
pub fn export(state: &AppState) {
    let Ok(vault) = state.vault.lock() else { return };
    let Some(vault) = vault.as_ref() else { return };
    let Ok(store) = state.store.lock() else { return };
    match store.snapshot().map(|snapshot| vault.write(&snapshot)) {
        Ok(Ok(_)) => {}
        Ok(Err(error)) => eprintln!("could not write the vault: {error}"),
        Err(error) => eprintln!("could not read the store: {error}"),
    }
}

/// Rebuilds the database from the files. Returns whether it ran.
pub fn import(state: &AppState) -> bool {
    let Ok(vault) = state.vault.lock() else { return false };
    let Some(vault) = vault.as_ref() else { return false };
    let today = chrono::Local::now().date_naive();

    let snapshot = match vault.read(today) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            eprintln!("could not read the vault: {error}");
            return false;
        }
    };
    let Ok(mut store) = state.store.lock() else { return false };
    if let Err(error) = store.restore(&snapshot) {
        eprintln!("could not rebuild from the vault: {error}");
        return false;
    }
    true
}

/// Tells every window the task list moved under it, and refreshes the tray
/// tooltip so a hover is never stale.
pub fn notify_changed(app: &AppHandle) {
    let _ = app.emit(DATA_CHANGED, ());
    crate::reminders::update_tooltip(app);
}
