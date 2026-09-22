use chrono::{Duration, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::recur;
use crate::task::Task;

/// Which tasks a view shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Filter {
    /// Everything not yet done.
    All,
    /// Due today, including a recurring task with an occurrence today. Overdue
    /// work is included: it is what the user most needs to see.
    Today,
    /// Past its due date and still open.
    Overdue,
    /// Open and falling within the next `days` days, today included.
    Next {
        days: u32,
    },
    /// Open with no project assigned.
    Inbox,
    Project(String),
    Tag(String),
    /// Case-insensitive substring over title, notes, and subtask titles.
    Search(String),
    Completed,
}

impl Filter {
    /// Whether `task` belongs in this view on `today`.
    pub fn matches(&self, task: &Task, today: NaiveDate) -> bool {
        // Completed work is out of every view but the one that asks for it.
        if task.is_done() {
            return matches!(self, Filter::Completed);
        }
        match self {
            Filter::All => true,
            Filter::Completed => false,
            Filter::Today => task.is_overdue(today) || falls_within(task, today, today),
            Filter::Overdue => task.is_overdue(today),
            Filter::Next { days } => {
                let end = today + Duration::days(i64::from(*days).saturating_sub(1).max(0));
                task.is_overdue(today) || falls_within(task, today, end)
            }
            Filter::Inbox => task.project.is_none(),
            Filter::Project(name) => {
                task.project.as_deref().is_some_and(|p| p.eq_ignore_ascii_case(name))
            }
            Filter::Tag(tag) => task.has_tag(tag),
            Filter::Search(needle) => matches_text(task, needle),
        }
    }

    /// A short label for the view, for tabs and window titles.
    pub fn label(&self) -> String {
        match self {
            Filter::All => "All".into(),
            Filter::Today => "Today".into(),
            Filter::Overdue => "Overdue".into(),
            Filter::Next { days } => format!("Next {days} days"),
            Filter::Inbox => "Inbox".into(),
            Filter::Project(name) => name.clone(),
            Filter::Tag(tag) => format!("#{tag}"),
            Filter::Search(needle) => format!("Search: {needle}"),
            Filter::Completed => "Completed".into(),
        }
    }
}

/// Whether the task lands anywhere in `from..=to` — for a recurring task, that
/// means any occurrence, not just the stored anchor.
fn falls_within(task: &Task, from: NaiveDate, to: NaiveDate) -> bool {
    if task.is_recurring() {
        !recur::occurrences(task, from, to).is_empty()
    } else {
        task.due.is_some_and(|due| (from..=to).contains(&due))
    }
}

fn matches_text(task: &Task, needle: &str) -> bool {
    let needle = needle.trim().to_lowercase();
    if needle.is_empty() {
        return true;
    }
    let haystacks = [task.title.as_str(), task.notes.as_str()];
    haystacks.iter().any(|h| h.to_lowercase().contains(&needle))
        || task.subtasks.iter().any(|s| s.title.to_lowercase().contains(&needle))
        || task.tags.iter().any(|t| t.contains(&needle))
        || task.project.as_deref().is_some_and(|p| p.to_lowercase().contains(&needle))
}

/// Narrowing applied on top of the agenda: a project, a tag, a search, or any
/// combination.
///
/// Separate from [`Filter`] because it deliberately says nothing about
/// completion — the agenda decides that on its own, so that searching with
/// completed work shown finds completed work too.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Narrow {
    pub project: Option<String>,
    pub tag: Option<String>,
    pub search: Option<String>,
}

impl Narrow {
    /// Whether every narrowing term present accepts this task.
    pub fn matches(&self, task: &Task) -> bool {
        let project_ok = self.project.as_ref().is_none_or(|name| {
            task.project.as_deref().is_some_and(|p| p.eq_ignore_ascii_case(name))
        });
        let tag_ok = self.tag.as_ref().is_none_or(|tag| task.has_tag(tag));
        let search_ok = self.search.as_ref().is_none_or(|needle| matches_text(task, needle));
        project_ok && tag_ok && search_ok
    }

    pub fn is_empty(&self) -> bool {
        self.project.is_none()
            && self.tag.is_none()
            && self.search.as_ref().is_none_or(|s| s.trim().is_empty())
    }
}

/// How a view orders its tasks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Sort {
    /// Soonest first, undated last, then by priority.
    #[default]
    Due,
    /// Most urgent first, then by due date.
    Priority,
    Created,
    Title,
}

impl Sort {
    /// Orders `tasks` in place. Sorting is stable, so equal keys keep the order
    /// the store returned them in.
    pub fn apply(self, tasks: &mut [Task], today: NaiveDate) {
        match self {
            Sort::Due => tasks.sort_by_key(|t| (due_key(t, today), t.priority)),
            Sort::Priority => tasks.sort_by_key(|t| (t.priority, due_key(t, today))),
            Sort::Created => tasks.sort_by_key(|t| t.created_at),
            Sort::Title => tasks.sort_by_key(|t| t.title.to_lowercase()),
        }
    }
}

/// Sort key that puts dated tasks in date order and undated ones last, with the
/// time of day breaking ties within a day.
fn due_key(task: &Task, today: NaiveDate) -> (bool, Option<NaiveDate>, Option<chrono::NaiveTime>) {
    let next = if task.is_recurring() {
        recur::next_occurrence(task, today).or(task.due)
    } else {
        task.due
    };
    (next.is_none(), next, task.time)
}
