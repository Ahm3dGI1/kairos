//! The agenda: one list, grouped by when things are actually due.
//!
//! This replaces a row of view tabs. Switching between Today, Next 7 Days and
//! Overdue made the user do the sorting — the answer to "what should I be doing"
//! was spread across three clicks. One scrolling list, ordered from most urgent
//! to least, answers it in one glance, and the buckets are headings rather than
//! destinations.
//!
//! The bucketing lives here rather than in a client so the widget, the Windows
//! list and a future TUI all agree on what "this week" means.

use chrono::{Duration, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::recur;
use crate::task::Task;

/// A section of the agenda, in the order they are shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Bucket {
    /// Past its date and still open. First, because it is the only group that
    /// represents a promise already broken.
    Overdue,
    Today,
    Tomorrow,
    /// Within the coming week, past tomorrow.
    ThisWeek,
    /// Dated, but further out than a week.
    Later,
    /// No date at all.
    Someday,
    /// Only present when completed work was asked for.
    Completed,
}

/// How far out `ThisWeek` reaches.
const WEEK_DAYS: i64 = 7;

impl Bucket {
    pub fn label(self) -> &'static str {
        match self {
            Bucket::Overdue => "Overdue",
            Bucket::Today => "Today",
            Bucket::Tomorrow => "Tomorrow",
            Bucket::ThisWeek => "Next 7 days",
            Bucket::Later => "Later",
            Bucket::Someday => "Someday",
            Bucket::Completed => "Completed",
        }
    }

    /// A stable machine-readable name, for a client keying off the group.
    pub fn id(self) -> &'static str {
        match self {
            Bucket::Overdue => "overdue",
            Bucket::Today => "today",
            Bucket::Tomorrow => "tomorrow",
            Bucket::ThisWeek => "week",
            Bucket::Later => "later",
            Bucket::Someday => "someday",
            Bucket::Completed => "completed",
        }
    }

    /// Which section a task belongs in on `today`.
    pub fn of(task: &Task, today: NaiveDate) -> Bucket {
        if task.is_done() {
            return Bucket::Completed;
        }
        // Overdue is decided by the stored date, not the next occurrence: a
        // daily task last done a week ago is behind, even though its rule would
        // happily offer today as the next one.
        if task.is_overdue(today) {
            return Bucket::Overdue;
        }
        let Some(date) = effective_date(task, today) else {
            return Bucket::Someday;
        };
        if date == today {
            Bucket::Today
        } else if Some(date) == today.succ_opt() {
            Bucket::Tomorrow
        } else if date <= today + Duration::days(WEEK_DAYS) {
            Bucket::ThisWeek
        } else {
            Bucket::Later
        }
    }
}

/// The date a task actually lands on next — the recurrence rule's answer for a
/// repeating task, the stored date otherwise.
pub fn effective_date(task: &Task, today: NaiveDate) -> Option<NaiveDate> {
    if task.is_recurring() {
        recur::next_occurrence(task, today).or(task.due)
    } else {
        task.due
    }
}

/// One section of the agenda.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Group {
    pub bucket: Bucket,
    pub tasks: Vec<Task>,
}

/// Groups tasks into the agenda, dropping empty sections.
///
/// Completed work is left out unless `include_completed` asks for it — it is a
/// thing you occasionally want to check, not part of what is ahead of you.
pub fn group(tasks: Vec<Task>, today: NaiveDate, include_completed: bool) -> Vec<Group> {
    const ORDER: [Bucket; 7] = [
        Bucket::Overdue,
        Bucket::Today,
        Bucket::Tomorrow,
        Bucket::ThisWeek,
        Bucket::Later,
        Bucket::Someday,
        Bucket::Completed,
    ];

    let mut groups: Vec<Group> =
        ORDER.iter().map(|&bucket| Group { bucket, tasks: Vec::new() }).collect();

    for task in tasks {
        let bucket = Bucket::of(&task, today);
        if bucket == Bucket::Completed && !include_completed {
            continue;
        }
        if let Some(group) = groups.iter_mut().find(|g| g.bucket == bucket) {
            group.tasks.push(task);
        }
    }

    // Within a section, soonest first, then by priority — the same order the
    // sections themselves follow.
    for group in &mut groups {
        group.tasks.sort_by_key(|t| (effective_date(t, today), t.time, t.priority));
    }
    groups.retain(|g| !g.tasks.is_empty());
    groups
}
