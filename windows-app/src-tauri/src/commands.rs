//! The commands the frontend calls.
//!
//! Every one of these is a thin shell over `mtodo-core`: parse a line, read a
//! view, write a change, emit "tasks-changed". No task semantics live here —
//! that is the core's job, and duplicating any of it would put the Windows app
//! and a future Linux client out of step.

use chrono::{Datelike, Local, NaiveDate};
use mtodo_core::{parse_excluding, recur, Field, Filter, Priority, Sort, Store, Task, TaskId};
use serde::{Deserialize, Serialize};
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
    f: impl FnOnce(&mut Store) -> Result<T, mtodo_core::StoreError>,
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
    f: impl FnOnce(&Store) -> Result<T, mtodo_core::StoreError>,
) -> CmdResult<T> {
    let store = state.store.lock().map_err(|_| "store lock poisoned".to_string())?;
    f(&store).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_tasks(
    state: State<'_, AppState>,
    filter: Filter,
    sort: Sort,
) -> CmdResult<Vec<TaskView>> {
    let today = today();
    let tasks = read_store(&state, |s| s.query(&filter, sort, today))?;
    Ok(tasks.into_iter().map(|t| TaskView::new(t, today)).collect())
}

#[tauri::command]
pub fn get_task(state: State<'_, AppState>, id: TaskId) -> CmdResult<Option<TaskView>> {
    let today = today();
    let task = read_store(&state, |s| s.get(id))?;
    Ok(task.map(|t| TaskView::new(t, today)))
}

/// A stretch of the input the parser claimed, so the capture field can outline
/// it and hand it back to be reverted.
#[derive(Serialize)]
pub struct PreviewSpan {
    /// "date", "time", "recurrence", "tag", "project" or "priority".
    field: &'static str,
    start: usize,
    end: usize,
    text: String,
}

fn field_name(field: Field) -> &'static str {
    match field {
        Field::Title => "title",
        Field::Date => "date",
        Field::Time => "time",
        Field::Recurrence => "recurrence",
        Field::Tag => "tag",
        Field::Project => "project",
        Field::Priority => "priority",
    }
}

/// What the parser made of a line, for the live preview under the input.
#[derive(Serialize)]
pub struct Preview {
    title: String,
    due: Option<NaiveDate>,
    time: Option<String>,
    recurrence_label: Option<String>,
    priority: String,
    project: Option<String>,
    tags: Vec<String>,
    /// Inferences worth showing, e.g. that "5" was read as 17:00.
    guesses: Vec<String>,
    /// Where each field came from, in byte offsets into the line.
    spans: Vec<PreviewSpan>,
}

/// Byte ranges the user has reverted to plain text, as [start, end] pairs.
type Excluded = Vec<(usize, usize)>;

fn ranges(excluded: &Excluded) -> Vec<std::ops::Range<usize>> {
    excluded.iter().map(|(start, end)| *start..*end).collect()
}

#[tauri::command]
pub fn preview_line(line: String, excluded: Option<Excluded>) -> Preview {
    let excluded = excluded.unwrap_or_default();
    let result = parse_excluding(&line, Local::now().naive_local(), &ranges(&excluded));
    let task = &result.task;
    Preview {
        title: task.title.clone(),
        due: task.due,
        time: task.time.map(|t| t.format("%H:%M").to_string()),
        recurrence_label: task.recurrence.map(|r| r.describe()),
        priority: task.priority.label().to_string(),
        project: task.project.clone(),
        tags: task.tags.clone(),
        guesses: result.guesses.iter().map(|g| g.note.clone()).collect(),
        spans: result
            .matches
            .iter()
            .map(|m| PreviewSpan {
                field: field_name(m.field),
                start: m.span.start,
                end: m.span.end,
                text: m.text.clone(),
            })
            .collect(),
    }
}

/// The headline interaction: one line in, a structured task out.
#[tauri::command]
pub fn quick_add(
    app: AppHandle,
    state: State<'_, AppState>,
    line: String,
    excluded: Option<Excluded>,
) -> CmdResult<Option<TaskView>> {
    if line.trim().is_empty() {
        return Ok(None);
    }
    // The same exclusions the preview used, so what is committed is exactly
    // what the capture field was showing.
    let excluded = ranges(&excluded.unwrap_or_default());
    let task = parse_excluding(&line, Local::now().naive_local(), &excluded).task;
    // A line that parses to nothing but structure still needs a name.
    let task = if task.title.trim().is_empty() {
        let mut task = task;
        task.title = line.trim().to_string();
        task
    } else {
        task
    };
    let today = today();
    with_store(&app, &state, |s| s.save(&task))?;
    Ok(Some(TaskView::new(task, today)))
}

#[tauri::command]
pub fn complete_task(
    app: AppHandle,
    state: State<'_, AppState>,
    id: TaskId,
) -> CmdResult<TaskView> {
    let today = today();
    let task = with_store(&app, &state, |s| s.complete(id, today))?;
    Ok(TaskView::new(task, today))
}

#[tauri::command]
pub fn uncomplete_task(
    app: AppHandle,
    state: State<'_, AppState>,
    id: TaskId,
) -> CmdResult<TaskView> {
    let today = today();
    let task = with_store(&app, &state, |s| s.uncomplete(id))?;
    Ok(TaskView::new(task, today))
}

#[tauri::command]
pub fn delete_task(app: AppHandle, state: State<'_, AppState>, id: TaskId) -> CmdResult<()> {
    with_store(&app, &state, |s| s.delete(id))
}

/// Applies an edit made in the detail pane.
#[derive(Deserialize)]
pub struct Edit {
    id: TaskId,
    title: Option<String>,
    notes: Option<String>,
    due: Option<Option<NaiveDate>>,
    time: Option<Option<String>>,
    priority: Option<String>,
    project: Option<Option<String>>,
    tags: Option<Vec<String>>,
    /// A recurrence phrase ("every monday"), or null to clear the rule.
    recurrence: Option<Option<String>>,
    /// "all" or "one-per-occurrence".
    checklist: Option<String>,
}

#[tauri::command]
pub fn update_task(app: AppHandle, state: State<'_, AppState>, edit: Edit) -> CmdResult<TaskView> {
    let today = today();
    let task = with_store(&app, &state, |store| {
        let mut task = store.get(edit.id)?.ok_or(mtodo_core::StoreError::NotFound(edit.id))?;
        if let Some(title) = edit.title {
            task.title = title;
        }
        if let Some(notes) = edit.notes {
            task.notes = notes;
        }
        if let Some(due) = edit.due {
            task.due = due;
        }
        if let Some(time) = edit.time {
            task.time = time.and_then(|t| chrono::NaiveTime::parse_from_str(&t, "%H:%M").ok());
        }
        if let Some(priority) = edit.priority {
            task.priority = Priority::parse(&priority).unwrap_or(Priority::None);
        }
        if let Some(project) = edit.project {
            task.project = project.filter(|p| !p.trim().is_empty());
        }
        if let Some(tags) = edit.tags {
            task.tags.clear();
            for tag in tags {
                task.add_tag(tag);
            }
        }
        if let Some(mode) = edit.checklist {
            task.checklist = mtodo_core::Checklist::parse(&mode);
        }
        if let Some(phrase) = edit.recurrence {
            task.recurrence = phrase
                .filter(|p| !p.trim().is_empty())
                .and_then(|p| mtodo_core::recurrence_from_phrase(&p));
            // A rule needs somewhere to start counting from.
            if task.recurrence.is_some() && task.due.is_none() {
                task.due = Some(today);
            }
        }
        store.save(&task)?;
        Ok(task)
    })?;
    Ok(TaskView::new(task, today))
}

#[tauri::command]
pub fn add_subtask(
    app: AppHandle,
    state: State<'_, AppState>,
    id: TaskId,
    title: String,
) -> CmdResult<TaskView> {
    let today = today();
    let task = with_store(&app, &state, |store| {
        let mut task = store.get(id)?.ok_or(mtodo_core::StoreError::NotFound(id))?;
        task.subtasks.push(mtodo_core::Subtask::new(title.trim()));
        store.save(&task)?;
        Ok(task)
    })?;
    Ok(TaskView::new(task, today))
}

#[tauri::command]
pub fn toggle_subtask(
    app: AppHandle,
    state: State<'_, AppState>,
    id: TaskId,
    subtask_id: uuid::Uuid,
) -> CmdResult<TaskView> {
    let today = today();
    let task = with_store(&app, &state, |store| {
        let mut task = store.get(id)?.ok_or(mtodo_core::StoreError::NotFound(id))?;
        mtodo_core::complete_item(&mut task, subtask_id, today);
        store.save(&task)?;
        Ok(task)
    })?;
    Ok(TaskView::new(task, today))
}

/// Skips one occurrence of a series without disturbing the rule.
#[tauri::command]
pub fn skip_occurrence(
    app: AppHandle,
    state: State<'_, AppState>,
    id: TaskId,
    date: NaiveDate,
) -> CmdResult<TaskView> {
    let today = today();
    let task = with_store(&app, &state, |store| {
        let mut task = store.get(id)?.ok_or(mtodo_core::StoreError::NotFound(id))?;
        task.exceptions.push(mtodo_core::Exception::skip(date));
        // Move the anchor past the skipped date so the task leaves today's list.
        if task.due == Some(date) {
            task.due = recur::next_occurrence(&task, date).or(task.due);
        }
        store.save(&task)?;
        Ok(task)
    })?;
    Ok(TaskView::new(task, today))
}

/// Moves one occurrence to another date, leaving the series intact.
#[tauri::command]
pub fn reschedule_occurrence(
    app: AppHandle,
    state: State<'_, AppState>,
    id: TaskId,
    date: NaiveDate,
    to: NaiveDate,
) -> CmdResult<TaskView> {
    let today = today();
    let task = with_store(&app, &state, |store| {
        let mut task = store.get(id)?.ok_or(mtodo_core::StoreError::NotFound(id))?;
        task.exceptions.push(mtodo_core::Exception::move_to(date, to));
        store.save(&task)?;
        Ok(task)
    })?;
    Ok(TaskView::new(task, today))
}

/// What an undo reversed, so the toast can name it.
#[derive(Serialize)]
pub struct UndoResult {
    label: String,
    title: Option<String>,
}

#[tauri::command]
pub fn undo(app: AppHandle, state: State<'_, AppState>) -> CmdResult<Option<UndoResult>> {
    let undone = with_store(&app, &state, |s| s.undo())?;
    Ok(undone
        .map(|u| UndoResult { label: u.kind.label().to_string(), title: u.task.map(|t| t.title) }))
}

#[tauri::command]
pub fn redo(app: AppHandle, state: State<'_, AppState>) -> CmdResult<Option<UndoResult>> {
    let redone = with_store(&app, &state, |s| s.redo())?;
    Ok(redone
        .map(|u| UndoResult { label: u.kind.label().to_string(), title: u.task.map(|t| t.title) }))
}

#[tauri::command]
pub fn can_undo(state: State<'_, AppState>) -> CmdResult<bool> {
    read_store(&state, |s| s.can_undo())
}

#[tauri::command]
pub fn can_redo(state: State<'_, AppState>) -> CmdResult<bool> {
    read_store(&state, |s| s.can_redo())
}

#[tauri::command]
pub fn projects(state: State<'_, AppState>) -> CmdResult<Vec<String>> {
    read_store(&state, |s| s.projects())
}

#[tauri::command]
pub fn tags(state: State<'_, AppState>) -> CmdResult<Vec<String>> {
    read_store(&state, |s| s.tags())
}

/// One day of the calendar grid.
#[derive(Serialize)]
pub struct CalendarDay {
    date: NaiveDate,
    tasks: Vec<TaskView>,
}

/// The calendar view: every occurrence falling in a month, expanded.
#[tauri::command]
pub fn calendar_month(
    state: State<'_, AppState>,
    year: i32,
    month: u32,
) -> CmdResult<Vec<CalendarDay>> {
    let today = today();
    let first = NaiveDate::from_ymd_opt(year, month, 1).ok_or("bad month")?;
    let last = last_day_of_month(year, month);

    let tasks = read_store(&state, |s| s.all())?;
    let open: Vec<Task> = tasks.into_iter().filter(|t| !t.is_done()).collect();

    let mut days = Vec::new();
    let mut date = first;
    while date <= last {
        let on_this_day: Vec<TaskView> = open
            .iter()
            .filter(|t| !recur::occurrences(t, date, date).is_empty())
            .map(|t| TaskView::new(t.clone(), today))
            .collect();
        days.push(CalendarDay { date, tasks: on_this_day });
        match date.succ_opt() {
            Some(next) => date = next,
            None => break,
        }
    }
    Ok(days)
}

fn last_day_of_month(year: i32, month: u32) -> NaiveDate {
    let (next_year, next_month) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
    NaiveDate::from_ymd_opt(next_year, next_month, 1)
        .and_then(|d| d.pred_opt())
        .unwrap_or_else(|| NaiveDate::from_ymd_opt(year, month, 28).unwrap())
}

/// Today's date, so the frontend never disagrees with the backend about what
/// "today" means across a midnight boundary.
#[tauri::command]
pub fn today_date() -> String {
    today().format("%Y-%m-%d").to_string()
}

/// A one-line summary for the tray tooltip and the widget header.
#[tauri::command]
pub fn summary(state: State<'_, AppState>) -> CmdResult<String> {
    let today = today();
    let due = read_store(&state, |s| s.query(&Filter::Today, Sort::Due, today))?;
    let overdue = due.iter().filter(|t| t.is_overdue(today)).count();
    Ok(match (due.len(), overdue) {
        (0, _) => "Nothing due today".to_string(),
        (n, 0) => format!("{n} due today"),
        (n, o) => format!("{n} due today, {o} overdue"),
    })
}

/// How many open tasks each sidebar view holds, computed in one pass so the
/// sidebar costs one call rather than one per view.
#[tauri::command]
pub fn view_counts(
    state: State<'_, AppState>,
) -> CmdResult<std::collections::HashMap<String, usize>> {
    let today = today();
    let tasks = read_store(&state, |s| s.all())?;

    let views: [(&str, Filter); 5] = [
        ("today", Filter::Today),
        ("next7", Filter::Next { days: 7 }),
        ("all", Filter::All),
        ("overdue", Filter::Overdue),
        ("inbox", Filter::Inbox),
    ];
    let mut counts = std::collections::HashMap::new();
    for (name, filter) in views {
        let n = tasks.iter().filter(|t| filter.matches(t, today)).count();
        counts.insert(name.to_string(), n);
    }
    Ok(counts)
}

/// The current month, for the calendar's initial render.
#[tauri::command]
pub fn current_month() -> (i32, u32) {
    let now = today();
    (now.year(), now.month())
}
