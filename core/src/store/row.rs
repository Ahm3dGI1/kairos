use chrono::{NaiveDate, NaiveTime};
use uuid::Uuid;

use super::StoreError;
use crate::task::{Checklist, Exception, Priority, Recurrence, Subtask, Task};

/// The stored form of a task: every field already a SQLite-native type.
pub struct Row {
    pub id: String,
    pub title: String,
    pub notes: String,
    pub due: Option<String>,
    pub time: Option<String>,
    pub recurrence: Option<String>,
    pub recurrence_anchor: Option<String>,
    pub exceptions: String,
    pub priority: String,
    pub tags: String,
    pub project: Option<String>,
    pub subtasks: String,
    pub completed_at: Option<String>,
    pub created_at: String,
    pub checklist: String,
    pub habit: Option<String>,
}

/// Dates are stored ISO-8601 so they sort correctly as text in SQL.
const DATE: &str = "%Y-%m-%d";
const TIME: &str = "%H:%M:%S";

impl Row {
    pub fn from_task(task: &Task) -> Result<Self, StoreError> {
        Ok(Self {
            id: task.id.to_string(),
            title: task.title.clone(),
            notes: task.notes.clone(),
            due: task.due.map(|d| d.format(DATE).to_string()),
            time: task.time.map(|t| t.format(TIME).to_string()),
            recurrence_anchor: task.recurrence_anchor.map(|d| d.to_string()),
            recurrence: task.recurrence.map(|r| serde_json::to_string(&r)).transpose()?,
            exceptions: serde_json::to_string(&task.exceptions)?,
            priority: task.priority.label().to_string(),
            tags: serde_json::to_string(&task.tags)?,
            project: task.project.clone(),
            subtasks: serde_json::to_string(&task.subtasks)?,
            completed_at: task.completed_at.map(|d| d.format(DATE).to_string()),
            created_at: task.created_at.format(DATE).to_string(),
            checklist: task.checklist.label().to_string(),
            habit: task.habit.map(|id| id.to_string()),
        })
    }

    pub fn from_sql(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            title: row.get("title")?,
            notes: row.get("notes")?,
            due: row.get("due")?,
            time: row.get("time")?,
            recurrence: row.get("recurrence")?,
            recurrence_anchor: row.get("recurrence_anchor")?,
            exceptions: row.get("exceptions")?,
            priority: row.get("priority")?,
            tags: row.get("tags")?,
            project: row.get("project")?,
            subtasks: row.get("subtasks")?,
            completed_at: row.get("completed_at")?,
            created_at: row.get("created_at")?,
            checklist: row.get("checklist")?,
            habit: row.get("habit")?,
        })
    }

    pub fn into_task(self) -> Result<Task, StoreError> {
        let recurrence: Option<Recurrence> =
            self.recurrence.map(|r| serde_json::from_str(&r)).transpose()?;
        let exceptions: Vec<Exception> = serde_json::from_str(&self.exceptions)?;
        let tags: Vec<String> = serde_json::from_str(&self.tags)?;
        let subtasks: Vec<Subtask> = serde_json::from_str(&self.subtasks)?;

        Ok(Task {
            // A row whose id or created_at will not parse is corrupt beyond
            // what a default can paper over, so fall back to values that keep
            // the rest of the task readable rather than dropping it entirely.
            id: Uuid::parse_str(&self.id).unwrap_or_else(|_| Uuid::new_v4()),
            title: self.title,
            notes: self.notes,
            due: parse_date(self.due.as_deref()),
            time: self.time.as_deref().and_then(|t| NaiveTime::parse_from_str(t, TIME).ok()),
            recurrence,
            recurrence_anchor: parse_date(self.recurrence_anchor.as_deref()),
            exceptions,
            priority: Priority::parse(&self.priority).unwrap_or_default(),
            tags,
            project: self.project,
            subtasks,
            checklist: Checklist::parse(&self.checklist),
            habit: self.habit.as_deref().and_then(|id| Uuid::parse_str(id).ok()),
            completed_at: parse_date(self.completed_at.as_deref()),
            created_at: parse_date(Some(&self.created_at)).unwrap_or_else(today_fallback),
        })
    }
}

fn parse_date(text: Option<&str>) -> Option<NaiveDate> {
    text.and_then(|d| NaiveDate::parse_from_str(d, DATE).ok())
}

fn today_fallback() -> NaiveDate {
    chrono::Local::now().date_naive()
}
