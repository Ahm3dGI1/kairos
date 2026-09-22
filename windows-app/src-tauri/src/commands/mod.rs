//! The commands the frontend calls.
//!
//! Every one of these is a thin shell over `kairos-core`: parse a line, read a
//! view, write a change, announce it. No task semantics live here — that is
//! the core's job, and duplicating any of it would put the Windows app and a
//! future Linux client out of step.
//!
//! This module holds what they share; each page's commands live beside it.

pub mod daily;
pub mod prayer;
pub mod settings;
pub mod tasks;
pub mod workout;

use chrono::{Local, NaiveDate};
use kairos_core::{recur, Store, Task};
use serde::Serialize;
use tauri::{AppHandle, State};

use crate::state::{notify_changed, AppState};

/// Commands report failure as a string: the frontend shows it and moves on,
/// and there is nothing it could do differently with a richer type.
pub type CmdResult<T> = Result<T, String>;

pub fn today() -> NaiveDate {
    Local::now().date_naive()
}

/// A task plus the things the UI would otherwise recompute per row.
#[derive(Serialize)]
pub struct TaskView {
    #[serde(flatten)]
    task: Task,
    /// The next date this actually happens, following the recurrence rule.
    next: Option<NaiveDate>,
    overdue: bool,
    /// "every other friday", for the row's subtitle.
    recurrence_label: Option<String>,
    /// How many subtasks are done, for the "2/5" badge.
    subtasks_done: usize,
    /// "all" or "one-per-occurrence".
    checklist: &'static str,
}

impl TaskView {
    pub fn new(task: Task, today: NaiveDate) -> Self {
        Self {
            next: if task.is_recurring() {
                recur::next_occurrence(&task, today).or(task.due)
            } else {
                task.due
            },
            overdue: task.is_overdue(today),
            recurrence_label: task.recurrence.map(|r| r.describe()),
            subtasks_done: task.subtasks.iter().filter(|s| s.done).count(),
            checklist: task.checklist.label(),
            task,
        }
    }
}

/// Runs `f` with the store locked, then tells every window to refresh.
pub fn with_store<T>(
    app: &AppHandle,
    state: &State<'_, AppState>,
    f: impl FnOnce(&mut Store) -> Result<T, kairos_core::StoreError>,
) -> CmdResult<T> {
    let mut store = state.store.lock().map_err(|_| "store lock poisoned".to_string())?;
    let out = f(&mut store).map_err(|e| e.to_string())?;
    drop(store);
    // The files are the record, so a change is not really made until they say
    // so. This is the one place every mutation passes through, which is why
    // the mirror lives here rather than in each command.
    crate::state::export(state);
    notify_changed(app);
    Ok(out)
}

/// Reads from the store without announcing a change.
pub fn read_store<T>(
    state: &State<'_, AppState>,
    f: impl FnOnce(&Store) -> Result<T, kairos_core::StoreError>,
) -> CmdResult<T> {
    let store = state.store.lock().map_err(|_| "store lock poisoned".to_string())?;
    f(&store).map_err(|e| e.to_string())
}
