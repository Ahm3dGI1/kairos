//! Application state: one open store, shared across every window.
//!
//! The tray, the widget, and the quick-add overlay are separate webviews but
//! one app — they must see the same tasks the instant either changes them, so
//! they share a single [`Store`] rather than each opening the database.

use std::sync::Mutex;

use mtodo_core::Store;
use tauri::{AppHandle, Emitter, Manager};

pub struct AppState {
    pub store: Mutex<Store>,
}

/// Event every window listens for: something changed, re-read your view.
pub const TASKS_CHANGED: &str = "tasks-changed";

/// Opens the database under the user's app data directory and manages it.
pub fn init(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let dir = app.path().app_data_dir()?;
    std::fs::create_dir_all(&dir)?;
    let store = Store::open(dir.join("tasks.db"))?;
    app.manage(AppState { store: Mutex::new(store) });
    Ok(())
}

/// Tells every window the task list moved under it, and refreshes the tray
/// tooltip so a hover is never stale.
pub fn notify_changed(app: &AppHandle) {
    let _ = app.emit(TASKS_CHANGED, ());
    crate::reminders::update_tooltip(app);
}
