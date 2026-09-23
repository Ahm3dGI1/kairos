use kairos_core::{Setting, Settings};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use super::CmdResult;
use crate::state::{self, AppState, DATA_CHANGED};

/// Emitted when a switch moves, so every window re-reads rather than being
/// told which one changed.
pub const SETTINGS_CHANGED: &str = "settings-changed";

/// The current settings, in a shape the page can render directly.
#[tauri::command]
pub fn settings(state: State<'_, AppState>) -> CmdResult<Vec<Setting>> {
    let settings = state.settings.lock().map_err(|_| "settings lock poisoned")?;
    Ok(settings.describe())
}

/// The raw values, for the parts of the UI that just need to know.
#[tauri::command]
pub fn settings_values(state: State<'_, AppState>) -> CmdResult<Settings> {
    let settings = state.settings.lock().map_err(|_| "settings lock poisoned")?;
    Ok(settings.clone())
}

#[derive(Serialize)]
pub struct VaultInfo {
    path: String,
    enabled: bool,
    /// Present, and holding files, right now.
    exists: bool,
    files: usize,
    settings_file: String,
}

#[tauri::command]
pub fn vault_info(state: State<'_, AppState>) -> CmdResult<VaultInfo> {
    let vault = state.vault.lock().map_err(|_| "vault lock poisoned")?;
    let root = state.paths.vault_dir.clone();

    let mut files = 0;
    for dir in ["tasks", "habits", "workouts"] {
        if let Ok(entries) = std::fs::read_dir(root.join(dir)) {
            files += entries
                .flatten()
                .filter(|e| e.path().extension().is_some_and(|ext| ext.eq_ignore_ascii_case("md")))
                .count();
        }
    }

    Ok(VaultInfo {
        path: root.to_string_lossy().to_string(),
        enabled: vault.is_some(),
        exists: vault.as_ref().is_some_and(|v| v.exists()),
        files,
        settings_file: state.paths.settings_file().to_string_lossy().to_string(),
    })
}

/// Sets one switch and writes the file.
#[tauri::command]
pub fn set_setting(
    app: AppHandle,
    state: State<'_, AppState>,
    key: String,
    value: serde_json::Value,
) -> CmdResult<Vec<Setting>> {
    let _guard = state.io.lock().map_err(|_| "storage lock poisoned")?;
    let mut settings = state.settings.lock().map_err(|_| "settings lock poisoned")?;
    let mut updated = settings.clone();
    if !updated.set(&key, value) {
        return Err(format!("{key} is not a setting, or that is not a value it takes"));
    }
    if key == "start_on_login" {
        crate::shell::sync_autostart(&app, updated.start_on_login)?;
    }
    if let Err(error) = updated.save(state.paths.settings_file()) {
        if key == "start_on_login" {
            let _ = crate::shell::sync_autostart(&app, settings.start_on_login);
        }
        return Err(error.to_string());
    }
    let described = updated.describe();
    *settings = updated;
    if key == "sticky_note" && !settings.sticky_note {
        if let Some(window) = app.get_webview_window("widget") {
            let _ = window.hide();
        }
    }
    drop(settings);

    let _ = app.emit(SETTINGS_CHANGED, ());
    let _ = app.emit(DATA_CHANGED, ());
    Ok(described)
}

/// Rebuilds the database from the files, now, rather than waiting for the
/// watcher. The button exists for the case where you have just edited a lot of
/// files and want to see it land.
#[tauri::command]
pub fn reload_vault(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    state::import(&state)?;
    state::notify_changed(&app);
    Ok(())
}

/// Writes every file out again, whatever their current contents.
#[tauri::command]
pub fn rewrite_vault(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    state::export(&state)?;
    state::notify_changed(&app);
    Ok(())
}

/// Shows the vault in Explorer.
#[tauri::command]
pub fn open_vault(state: State<'_, AppState>) -> CmdResult<()> {
    let path = state.paths.vault_dir.clone();
    std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    std::process::Command::new("explorer")
        .arg(path)
        // Explorer returns a non-zero exit code even when it works, so the
        // status is not worth checking.
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Reads the settings the shell needs before any window exists.
pub fn current(app: &AppHandle) -> Settings {
    app.try_state::<AppState>()
        .and_then(|state| state.settings.lock().ok().map(|s| s.clone()))
        .unwrap_or_default()
}
