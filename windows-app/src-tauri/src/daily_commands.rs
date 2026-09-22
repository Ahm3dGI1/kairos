//! Commands for the two new surfaces: the grouped agenda, and the habit month
//! — a grid of habits by day, with the month's journal beside it.
//!
//! Like the rest of the shell, these are wrappers. The bucketing rules live in
//! `kairos_core::agenda` and the habit rules in `kairos_core::daily`.

use chrono::{Datelike, Local, NaiveDate};
use kairos_core::{agenda, daily, DayLog, HabitId, JournalEntry, MonthJournal, Narrow, Task};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::commands::{read_store, today, with_store, CmdResult, TaskView};
use crate::state::AppState;

// ---------- the agenda ----------

/// What the agenda should show: everything ahead, optionally narrowed, with
/// completed work folded in only when asked for.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct AgendaQuery {
    pub include_completed: bool,
    pub search: Option<String>,
    pub project: Option<String>,
    pub tag: Option<String>,
}

/// One section of the agenda, ready to render.
#[derive(Serialize)]
pub struct GroupView {
    /// Stable key, e.g. "overdue".
    id: &'static str,
    label: &'static str,
    tasks: Vec<TaskView>,
}

#[tauri::command]
pub fn agenda(state: State<'_, AppState>, query: AgendaQuery) -> CmdResult<Vec<GroupView>> {
    let today = today();
    let narrow = Narrow {
        project: query.project.filter(|p| !p.trim().is_empty()),
        tag: query.tag.filter(|t| !t.trim().is_empty()),
        search: query.search.filter(|s| !s.trim().is_empty()),
    };

    let tasks: Vec<Task> = read_store(&state, |store| store.all())?
        .into_iter()
        .filter(|t| narrow.matches(t))
        .collect();

    Ok(agenda::group(tasks, today, query.include_completed)
        .into_iter()
        .map(|group| GroupView {
            id: group.bucket.id(),
            label: group.bucket.label(),
            tasks: group.tasks.into_iter().map(|t| TaskView::new(t, today)).collect(),
        })
        .collect())
}

// ---------- the habit month ----------

/// One habit's row across the month.
#[derive(Serialize)]
pub struct HabitRow {
    id: HabitId,
    name: String,
    /// "check", "duration" or "number:pages".
    kind: String,
    /// Whether the row takes a number rather than a tick.
    numeric: bool,
    /// One entry per day of the month, in order. Empty for a numeric row.
    done: Vec<bool>,
    /// One entry per day for a numeric row, already formatted for the cell.
    values: Vec<String>,
    /// Days with something on them this month, for the row total.
    count: usize,
    /// The mean of the days that recorded a number, formatted. A total would
    /// be the wrong summary for a duration you are trying to hold steady.
    average: Option<String>,
    streak: u32,
}

/// One day column: the date, and whether it is in the future or today.
#[derive(Serialize)]
pub struct DayColumn {
    date: NaiveDate,
    day: u32,
    /// "M", "T", … so the header can show the weekday under the number.
    weekday: String,
    is_today: bool,
    is_future: bool,
    is_weekend: bool,
}

/// Everything the habit page renders for one month.
#[derive(Serialize)]
pub struct HabitMonth {
    year: i32,
    month: u32,
    /// "September 2026"
    label: String,
    days: Vec<DayColumn>,
    habits: Vec<HabitRow>,
    /// Retired habits, so archiving is not a one-way door.
    archived: Vec<HabitBrief>,
}

fn last_day_of_month(year: i32, month: u32) -> u32 {
    let (next_year, next_month) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
    NaiveDate::from_ymd_opt(next_year, next_month, 1)
        .and_then(|first| first.pred_opt())
        .map_or(28, |last| last.day())
}

const MONTH_NAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

#[tauri::command]
pub fn habit_month(state: State<'_, AppState>, year: i32, month: u32) -> CmdResult<HabitMonth> {
    let today = today();
    let first = NaiveDate::from_ymd_opt(year, month, 1).ok_or("bad month")?;
    let last =
        NaiveDate::from_ymd_opt(year, month, last_day_of_month(year, month)).ok_or("bad month")?;

    let (all, logs, streaks) = read_store(&state, |store| {
        Ok((store.habits(true)?, store.day_logs_between(first, last)?, store.habit_streaks(today)?))
    })?;
    let archived: Vec<HabitBrief> = all
        .iter()
        .filter(|h| h.archived)
        .map(|h| HabitBrief { id: h.id, name: h.name.clone(), kind: h.kind.label() })
        .collect();
    let habits: Vec<_> = all.into_iter().filter(|h| !h.archived).collect();

    let by_date: std::collections::HashMap<NaiveDate, &DayLog> =
        logs.iter().map(|log| (log.date, log)).collect();

    let dates: Vec<NaiveDate> = (0..last.day())
        .filter_map(|offset| first.checked_add_signed(chrono::Duration::days(offset as i64)))
        .collect();

    let days = dates
        .iter()
        .map(|date| DayColumn {
            date: *date,
            day: date.day(),
            weekday: date.format("%a").to_string()[..1].to_string(),
            is_today: *date == today,
            is_future: *date > today,
            is_weekend: matches!(date.weekday(), chrono::Weekday::Sat | chrono::Weekday::Sun),
        })
        .collect();

    let habit_rows = habits
        .into_iter()
        .map(|habit| {
            let numeric = habit.kind.is_numeric();
            let raw: Vec<Option<f64>> = dates
                .iter()
                .map(|date| by_date.get(date).and_then(|log| log.value(habit.id)))
                .collect();

            let done: Vec<bool> = if numeric {
                Vec::new()
            } else {
                dates
                    .iter()
                    .map(|date| by_date.get(date).is_some_and(|log| log.is_done(habit.id)))
                    .collect()
            };
            let values: Vec<String> = if numeric {
                raw.iter().map(|v| v.map(|v| habit.kind.format(v)).unwrap_or_default()).collect()
            } else {
                Vec::new()
            };

            let recorded: Vec<f64> = raw.iter().flatten().copied().collect();
            let average = (!recorded.is_empty())
                .then(|| habit.kind.format(recorded.iter().sum::<f64>() / recorded.len() as f64));

            HabitRow {
                count: if numeric { recorded.len() } else { done.iter().filter(|d| **d).count() },
                done,
                values,
                average,
                numeric,
                kind: habit.kind.label(),
                streak: streaks.get(&habit.id).copied().unwrap_or(0),
                id: habit.id,
                name: habit.name,
            }
        })
        .collect();

    Ok(HabitMonth {
        label: format!("{} {year}", MONTH_NAMES[(month.max(1) - 1) as usize % 12]),
        year,
        month,
        days,
        habits: habit_rows,
        archived,
    })
}

#[tauri::command]
pub fn toggle_habit(
    app: AppHandle,
    state: State<'_, AppState>,
    id: HabitId,
    date: NaiveDate,
) -> CmdResult<bool> {
    // Ticking a day that has not happened yet is always a misclick.
    if date > today() {
        return Ok(false);
    }
    with_store(&app, &state, |store| store.toggle_habit(id, date))
}

#[tauri::command]
pub fn add_habit(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    kind: Option<String>,
) -> CmdResult<()> {
    if name.trim().is_empty() {
        return Ok(());
    }
    let today = today();
    with_store(&app, &state, |store| {
        let mut habit = store.add_habit(&name, today)?;
        if let Some(kind) = kind {
            habit.kind = daily::HabitKind::parse(&kind);
            store.save_habit(&habit)?;
        }
        Ok(())
    })
}

/// Just the habits, for anywhere that needs to name one — the detail pane's
/// link picker, and the archived list on the habit page.
#[derive(Serialize)]
pub struct HabitBrief {
    id: HabitId,
    name: String,
    kind: String,
}

#[tauri::command]
pub fn habits(state: State<'_, AppState>) -> CmdResult<Vec<HabitBrief>> {
    let habits = read_store(&state, |store| store.habits(false))?;
    Ok(habits
        .into_iter()
        .map(|h| HabitBrief { id: h.id, name: h.name, kind: h.kind.label() })
        .collect())
}

#[tauri::command]
pub fn edit_habit(
    app: AppHandle,
    state: State<'_, AppState>,
    id: HabitId,
    name: Option<String>,
    kind: Option<String>,
    archived: Option<bool>,
) -> CmdResult<()> {
    with_store(&app, &state, |store| {
        let Some(mut habit) = store.habits(true)?.into_iter().find(|h| h.id == id) else {
            return Ok(());
        };
        if let Some(name) = name {
            // A habit with no name could not be found again, so an emptied
            // box leaves the name alone rather than applying itself.
            if !name.trim().is_empty() {
                habit.name = name.trim().to_string();
            }
        }
        // Existing entries are left as they are. A number read as a duration
        // is still the same number, and losing a month of history to a
        // mistaken tap would be worse than an odd-looking cell.
        if let Some(kind) = kind {
            habit.kind = daily::HabitKind::parse(&kind);
        }
        if let Some(archived) = archived {
            habit.archived = archived;
        }
        store.save_habit(&habit)
    })
}

#[tauri::command]
pub fn delete_habit(app: AppHandle, state: State<'_, AppState>, id: HabitId) -> CmdResult<()> {
    with_store(&app, &state, |store| store.delete_habit(id))
}

/// Sets one of the hand-entered numbers for a day, reading the loose shapes a
/// person types ("7h30", "7.5h", "450"). An empty value clears it.
#[tauri::command]
pub fn set_habit_value(
    app: AppHandle,
    state: State<'_, AppState>,
    date: NaiveDate,
    id: HabitId,
    value: String,
) -> CmdResult<()> {
    if date > today() {
        return Ok(());
    }
    with_store(&app, &state, |store| {
        let Some(habit) = store.habits(true)?.into_iter().find(|h| h.id == id) else {
            return Ok(());
        };
        let mut log = store.day_log(date)?;
        // An emptied cell is a day that recorded nothing, not a day that
        // recorded zero — the difference between "did not measure" and "none".
        log.set_value(id, habit.kind.parse_value(&value));
        store.save_day_log(&log)
    })
}

// ---------- the month journal ----------

#[tauri::command]
pub fn month_journal(state: State<'_, AppState>, year: i32, month: u32) -> CmdResult<MonthJournal> {
    read_store(&state, |store| store.month_journal(year, month))
}

/// Adds or updates one entry. A new entry arrives without an id.
#[derive(Deserialize)]
pub struct EntryEdit {
    id: Option<uuid::Uuid>,
    title: String,
    body: String,
}

#[tauri::command]
pub fn save_journal_entry(
    app: AppHandle,
    state: State<'_, AppState>,
    year: i32,
    month: u32,
    entry: EntryEdit,
) -> CmdResult<MonthJournal> {
    with_store(&app, &state, |store| {
        let mut journal = store.month_journal(year, month)?;
        match entry.id.and_then(|id| journal.entries.iter_mut().find(|e| e.id == id)) {
            Some(existing) => {
                existing.title = entry.title.clone();
                existing.body = entry.body.clone();
            }
            None => {
                let mut fresh = JournalEntry::new(entry.title.clone());
                fresh.body = entry.body.clone();
                journal.entries.push(fresh);
            }
        }
        store.save_month_journal(&journal)?;
        store.month_journal(year, month)
    })
}

#[tauri::command]
pub fn delete_journal_entry(
    app: AppHandle,
    state: State<'_, AppState>,
    year: i32,
    month: u32,
    id: uuid::Uuid,
) -> CmdResult<MonthJournal> {
    with_store(&app, &state, |store| {
        let mut journal = store.month_journal(year, month)?;
        journal.entries.retain(|entry| entry.id != id);
        store.save_month_journal(&journal)?;
        store.month_journal(year, month)
    })
}

/// The month to open the habit page on.
#[tauri::command]
pub fn current_month_pair() -> (i32, u32) {
    let now = Local::now().date_naive();
    (now.year(), now.month())
}
