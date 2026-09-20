//! Noticing that someone edited the vault behind the app's back.
//!
//! Deliberately a poll rather than a filesystem-event subscription. The vault
//! is meant to live wherever the user keeps their notes — a synced folder, a
//! network share, a git checkout — and change notifications on those are
//! unreliable in exactly the cases that matter. A fingerprint of a few dozen
//! small files costs nothing to take twice a second, and it cannot miss a
//! change the way a dropped event can.
//!
//! The app's own writes are seen too, but they cost nothing: the vault only
//! writes a file whose contents differ, so re-importing what we just exported
//! produces an identical database and an identical folder.

use std::time::Duration;

use tauri::{AppHandle, Manager};

use crate::state::{self, AppState};

/// How often to look. Fast enough that saving a file in an editor and
/// alt-tabbing back feels immediate, slow enough to be invisible.
const INTERVAL: Duration = Duration::from_millis(1500);

/// A cheap summary of the vault's contents: how many files, and the newest
/// modification time among them. Any edit moves one or the other.
type Fingerprint = (usize, u128);

pub fn start(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let mut last = match fingerprint(&app) {
            Some(print) => print,
            // No vault: nothing to watch, and turning one on needs a restart.
            None => return,
        };

        loop {
            std::thread::sleep(INTERVAL);
            let Some(current) = fingerprint(&app) else { continue };
            if current == last {
                continue;
            }
            last = current;

            let Some(state) = app.try_state::<AppState>() else { continue };
            if state::import(&state) {
                state::notify_changed(&app);
            }
            // Take the fingerprint again: importing may have rewritten files
            // (a hand-written line gaining an id, say), and that must not read
            // as another external edit on the next tick.
            if let Some(settled) = fingerprint(&app) {
                last = settled;
            }
        }
    });
}

fn fingerprint(app: &AppHandle) -> Option<Fingerprint> {
    let state = app.try_state::<AppState>()?;
    let vault = state.vault.lock().ok()?;
    let root = vault.as_ref()?.root().to_path_buf();
    drop(vault);

    let mut count = 0;
    let mut newest = 0u128;
    for dir in ["tasks", "habits", "workouts"] {
        let Ok(entries) = std::fs::read_dir(root.join(dir)) else { continue };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            if !meta.is_file() {
                continue;
            }
            count += 1;
            if let Ok(modified) = meta.modified() {
                if let Ok(since) = modified.duration_since(std::time::UNIX_EPOCH) {
                    newest = newest.max(since.as_millis());
                }
            }
        }
    }
    Some((count, newest))
}
