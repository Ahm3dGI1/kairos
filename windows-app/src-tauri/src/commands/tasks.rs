use chrono::{Datelike, Local, NaiveDate};
use kairos_core::{parse_excluding, recur, Field, Filter, Priority, Sort, Task, TaskId};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use super::{read_store, today, with_store, CmdResult, TaskView};
use crate::state::AppState;

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

/// Lets a field tell "was not sent" apart from "was sent as null".
///
/// Serde folds both onto `None` for a plain `Option<Option<T>>`, which would
/// make "leave the due date alone" and "clear the due date" the same request —
/// so every edit would have to send every field, and clearing one would be
/// impossible. Absent stays `None`; an explicit `null` becomes `Some(None)`.
fn sent<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

/// Applies an edit made in the detail pane.
///
/// Every field is optional and means "leave this alone" when absent. The four
/// that can be cleared take `null` to mean exactly that.
#[derive(Debug, Deserialize)]
pub struct Edit {
    id: TaskId,
    title: Option<String>,
    notes: Option<String>,
    #[serde(default, deserialize_with = "sent")]
    due: Option<Option<NaiveDate>>,
    #[serde(default, deserialize_with = "sent")]
    time: Option<Option<String>>,
    priority: Option<String>,
    #[serde(default, deserialize_with = "sent")]
    project: Option<Option<String>>,
    tags: Option<Vec<String>>,
    /// A recurrence phrase ("every monday and wednesday"), or null to clear it.
    #[serde(default, deserialize_with = "sent")]
    recurrence: Option<Option<String>>,
    /// "all" or "one-per-occurrence".
    checklist: Option<String>,
    /// The habit this task stands for, or null to unlink it.
    #[serde(default, deserialize_with = "sent")]
    habit: Option<Option<kairos_core::HabitId>>,
}

#[tauri::command]
pub fn update_task(app: AppHandle, state: State<'_, AppState>, edit: Edit) -> CmdResult<TaskView> {
    let today = today();
    let task = with_store(&app, &state, |store| {
        let mut task = store.get(edit.id)?.ok_or(kairos_core::StoreError::NotFound(edit.id))?;
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
        if let Some(habit) = edit.habit {
            task.habit = habit;
        }
        if let Some(mode) = edit.checklist {
            task.checklist = kairos_core::Checklist::parse(&mode);
        }
        if let Some(phrase) = edit.recurrence {
            task.recurrence = phrase
                .filter(|p| !p.trim().is_empty())
                .and_then(|p| kairos_core::recurrence_from_phrase(&p));
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
        let mut task = store.get(id)?.ok_or(kairos_core::StoreError::NotFound(id))?;
        task.subtasks.push(kairos_core::Subtask::new(title.trim()));
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
        let mut task = store.get(id)?.ok_or(kairos_core::StoreError::NotFound(id))?;
        kairos_core::complete_item(&mut task, subtask_id, today);
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
        let mut task = store.get(id)?.ok_or(kairos_core::StoreError::NotFound(id))?;
        task.exceptions.push(kairos_core::Exception::skip(date));
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
        let mut task = store.get(id)?.ok_or(kairos_core::StoreError::NotFound(id))?;
        task.exceptions.push(kairos_core::Exception::move_to(date, to));
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

/// The current month, for the calendar's initial render.
#[tauri::command]
pub fn current_month() -> (i32, u32) {
    let now = today();
    (now.year(), now.month())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "7f3a9c2e-4b1d-4f2a-9c3e-1a2b3c4d5e6f";

    fn edit(json: &str) -> Edit {
        serde_json::from_str(&format!(r#"{{"id":"{ID}",{json}}}"#)).expect("edit should parse")
    }

    /// The three states every clearable field has to carry. Collapsing the
    /// first two is what made "set a repeat" and "clear the repeat" the same
    /// request, and it fails silently — the field just never changes.
    #[test]
    fn absent_clear_and_set_are_three_different_things() {
        let untouched = edit(r#""title":"x""#);
        assert!(untouched.due.is_none(), "absent means leave it alone");
        assert!(untouched.recurrence.is_none());

        let cleared = edit(r#""due":null,"recurrence":null,"time":null"#);
        assert_eq!(cleared.due, Some(None), "null means clear it");
        assert_eq!(cleared.recurrence, Some(None));
        assert_eq!(cleared.time, Some(None));

        let set = edit(r#""due":"2026-09-21","recurrence":"every monday","time":"17:00""#);
        assert_eq!(set.due, Some(NaiveDate::from_ymd_opt(2026, 9, 21)));
        assert_eq!(set.recurrence, Some(Some("every monday".into())));
        assert_eq!(set.time, Some(Some("17:00".into())));
    }

    /// What the detail pane used to send. Keeping it rejected means the bug
    /// cannot come back quietly: it fails at the boundary, loudly.
    #[test]
    fn a_wrapped_value_is_rejected_rather_than_ignored() {
        let json = format!(r#"{{"id":"{ID}","recurrence":["every monday"]}}"#);
        let error = serde_json::from_str::<Edit>(&json).unwrap_err().to_string();
        assert!(error.contains("invalid type"), "got: {error}");
    }
}
