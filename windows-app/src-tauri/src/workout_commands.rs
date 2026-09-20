//! Commands for the workout book.
//!
//! The shape mirrors the spreadsheet it replaces: a routine is a page, its
//! exercises are the rows, and each session is a group of columns beside them.
//! Nothing here interprets the numbers — that is the point of recording them.

use chrono::{Datelike, NaiveDate};
use kairos_core::workout::{self, ExerciseId, RoutineId, SessionId, SetEntry};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::commands::{read_store, today, with_store, CmdResult};
use crate::state::AppState;

#[derive(Serialize)]
pub struct RoutineView {
    id: RoutineId,
    name: String,
    sessions: usize,
}

#[derive(Serialize)]
pub struct ExerciseView {
    id: ExerciseId,
    name: String,
}

#[derive(Serialize)]
pub struct SetView {
    exercise: ExerciseId,
    index: u32,
    reps: u32,
    weight: f64,
    /// Pre-formatted, so the grid never has to decide about trailing zeroes.
    weight_label: String,
}

#[derive(Serialize)]
pub struct SessionView {
    id: SessionId,
    date: NaiveDate,
    /// "Sat 19 Sep"
    label: String,
    note: String,
    sets: Vec<SetView>,
    is_today: bool,
}

/// Everything the workout page draws for one routine.
#[derive(Serialize)]
pub struct WorkoutPage {
    routines: Vec<RoutineView>,
    routine: Option<RoutineId>,
    name: String,
    exercises: Vec<ExerciseView>,
    /// Newest first, which is the order the page reads left to right.
    sessions: Vec<SessionView>,
    /// The widest run of sets on the page, so every exercise block lines up.
    max_sets: u32,
}

fn label_for(date: NaiveDate) -> String {
    const DAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    const MONTHS: [&str; 12] =
        ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    format!(
        "{} {} {}",
        DAYS[date.weekday().num_days_from_monday() as usize],
        date.day(),
        MONTHS[(date.month0() % 12) as usize]
    )
}

/// How many recent sessions the page shows at once. Older ones stay in the
/// database; the page is for comparing against last time, not for reading a year.
const RECENT: u32 = 8;

#[tauri::command]
pub fn workout_page(
    state: State<'_, AppState>,
    routine: Option<RoutineId>,
) -> CmdResult<WorkoutPage> {
    let today = today();
    read_store(&state, |store| {
        let routines = store.routines()?;
        let chosen = routine
            .filter(|id| routines.iter().any(|r| r.id == *id))
            .or(routines.first().map(|r| r.id));

        let mut views = Vec::new();
        for r in &routines {
            views.push(RoutineView {
                id: r.id,
                name: r.name.clone(),
                sessions: store.sessions(r.id, u32::MAX)?.len(),
            });
        }

        let Some(id) = chosen else {
            return Ok(WorkoutPage {
                routines: views,
                routine: None,
                name: String::new(),
                exercises: Vec::new(),
                sessions: Vec::new(),
                max_sets: 0,
            });
        };

        let exercises = store.exercises(id)?;
        let logs = store.sessions(id, RECENT)?;
        let max_sets =
            logs.iter().flat_map(|log| log.sets.iter().map(|s| s.index)).max().unwrap_or(0).max(3);

        let sessions = logs
            .into_iter()
            .map(|log| SessionView {
                id: log.session.id,
                date: log.session.date,
                label: label_for(log.session.date),
                note: log.session.note,
                is_today: log.session.date == today,
                sets: log
                    .sets
                    .into_iter()
                    .map(|set| SetView {
                        exercise: set.exercise,
                        index: set.index,
                        reps: set.reps,
                        weight: set.weight,
                        weight_label: workout::format_weight(set.weight),
                    })
                    .collect(),
            })
            .collect();

        Ok(WorkoutPage {
            name: routines.iter().find(|r| r.id == id).map(|r| r.name.clone()).unwrap_or_default(),
            routines: views,
            routine: Some(id),
            exercises: exercises
                .into_iter()
                .map(|e| ExerciseView { id: e.id, name: e.name })
                .collect(),
            sessions,
            max_sets,
        })
    })
}

#[tauri::command]
pub fn add_routine(app: AppHandle, state: State<'_, AppState>, name: String) -> CmdResult<()> {
    if name.trim().is_empty() {
        return Ok(());
    }
    with_store(&app, &state, |store| store.add_routine(&name).map(|_| ()))
}

#[tauri::command]
pub fn rename_routine(
    app: AppHandle,
    state: State<'_, AppState>,
    id: RoutineId,
    name: String,
) -> CmdResult<()> {
    if name.trim().is_empty() {
        return Ok(());
    }
    with_store(&app, &state, |store| {
        let Some(mut routine) = store.routines()?.into_iter().find(|r| r.id == id) else {
            return Ok(());
        };
        routine.name = name.trim().to_string();
        store.save_routine(&routine)
    })
}

#[tauri::command]
pub fn delete_routine(app: AppHandle, state: State<'_, AppState>, id: RoutineId) -> CmdResult<()> {
    with_store(&app, &state, |store| store.delete_routine(id))
}

#[tauri::command]
pub fn add_exercise(
    app: AppHandle,
    state: State<'_, AppState>,
    routine: RoutineId,
    name: String,
) -> CmdResult<()> {
    if name.trim().is_empty() {
        return Ok(());
    }
    with_store(&app, &state, |store| store.add_exercise(routine, &name).map(|_| ()))
}

#[tauri::command]
pub fn rename_exercise(
    app: AppHandle,
    state: State<'_, AppState>,
    routine: RoutineId,
    id: ExerciseId,
    name: String,
) -> CmdResult<()> {
    if name.trim().is_empty() {
        return Ok(());
    }
    with_store(&app, &state, |store| {
        let Some(mut exercise) = store.exercises(routine)?.into_iter().find(|e| e.id == id) else {
            return Ok(());
        };
        exercise.name = name.trim().to_string();
        store.save_exercise(&exercise)
    })
}

#[tauri::command]
pub fn delete_exercise(
    app: AppHandle,
    state: State<'_, AppState>,
    id: ExerciseId,
) -> CmdResult<()> {
    with_store(&app, &state, |store| store.delete_exercise(id))
}

/// Starts today's session, carrying last time's numbers across so the user
/// adjusts rather than retypes.
#[tauri::command]
pub fn start_session(
    app: AppHandle,
    state: State<'_, AppState>,
    routine: RoutineId,
) -> CmdResult<()> {
    let today = today();
    with_store(&app, &state, |store| store.start_session(routine, today).map(|_| ()))
}

#[derive(Deserialize)]
pub struct SetEdit {
    session: SessionId,
    exercise: ExerciseId,
    index: u32,
    reps: Option<u32>,
    /// Sent as typed; the core reads "60", "62.5", "60kg".
    weight: Option<String>,
}

#[tauri::command]
pub fn save_set(app: AppHandle, state: State<'_, AppState>, edit: SetEdit) -> CmdResult<()> {
    with_store(&app, &state, |store| {
        // An emptied row is a set that did not happen, so it goes away.
        let reps = edit.reps.unwrap_or(0);
        let weight = edit.weight.as_deref().and_then(workout::parse_weight).unwrap_or(0.0);
        if reps == 0 && weight == 0.0 {
            return store.delete_set(edit.session, edit.exercise, edit.index);
        }
        store.save_set(
            edit.session,
            SetEntry { exercise: edit.exercise, index: edit.index, reps, weight },
        )
    })
}

#[tauri::command]
pub fn save_session_note(
    app: AppHandle,
    state: State<'_, AppState>,
    session: SessionId,
    note: String,
) -> CmdResult<()> {
    with_store(&app, &state, |store| store.save_session_note(session, &note))
}

#[tauri::command]
pub fn delete_session(
    app: AppHandle,
    state: State<'_, AppState>,
    session: SessionId,
) -> CmdResult<()> {
    with_store(&app, &state, |store| store.delete_session(session))
}
