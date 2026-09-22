use chrono::{NaiveDate, NaiveTime, Weekday};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Stable identifier for a task. Generated on the device that created it, so a
/// task keeps its identity across sync without a server round-trip.
pub type TaskId = Uuid;

/// A task, however it was captured.
///
/// Only `title` is required: "buy milk" is a complete task. Every field is
/// public and mutable so a client can let the user override anything the parser
/// inferred — see [`ParseResult`](crate::ParseResult) for what was inferred.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    /// The input line with every recognized structural phrase removed.
    pub title: String,
    /// Free-form detail, never parsed.
    pub notes: String,
    /// Local calendar date. Naive by design: the shell supplies the timezone.
    /// On a recurring task this is the anchor — the first occurrence.
    pub due: Option<NaiveDate>,
    /// Local wall-clock time.
    pub time: Option<NaiveTime>,
    pub recurrence: Option<Recurrence>,
    /// Occurrences moved or skipped without breaking the series.
    pub exceptions: Vec<Exception>,
    pub priority: Priority,
    /// Lowercased, deduplicated, insertion-ordered.
    pub tags: Vec<String>,
    /// The project or section this belongs to, if any.
    pub project: Option<String>,
    pub subtasks: Vec<Subtask>,
    /// How the subtask list is read: a set of steps, or a backlog the series
    /// works through one occurrence at a time.
    #[serde(default)]
    pub checklist: Checklist,
    /// A habit this task stands for.
    ///
    /// "Gym" is both a thing to do five times a week and a thing to have a run
    /// of, and keeping them as two separate records means ticking both by
    /// hand. Completing a linked task ticks the habit for that day — one
    /// direction only, because completing is the act and the tick is its
    /// record, not the other way round.
    #[serde(default)]
    pub habit: Option<crate::daily::HabitId>,
    /// When this was completed. For a recurring task, completion advances
    /// [`Task::due`] to the next occurrence instead of setting this.
    pub completed_at: Option<NaiveDate>,
    pub created_at: NaiveDate,
}

impl Task {
    pub fn new(title: impl Into<String>, created_at: NaiveDate) -> Self {
        Self {
            id: Uuid::new_v4(),
            title: title.into(),
            notes: String::new(),
            due: None,
            time: None,
            recurrence: None,
            exceptions: Vec::new(),
            priority: Priority::None,
            tags: Vec::new(),
            project: None,
            subtasks: Vec::new(),
            checklist: Checklist::All,
            habit: None,
            completed_at: None,
            created_at,
        }
    }

    pub fn is_done(&self) -> bool {
        self.completed_at.is_some()
    }

    pub fn is_recurring(&self) -> bool {
        self.recurrence.is_some()
    }

    /// Whether this is past its due date on `today`. A task with no due date is
    /// never overdue, and a completed one stops being overdue.
    pub fn is_overdue(&self, today: NaiveDate) -> bool {
        !self.is_done() && self.due.is_some_and(|due| due < today)
    }

    /// Adds a tag if it is not already present. Tags are compared lowercased,
    /// so "#Work" and "#work" are one tag.
    pub fn add_tag(&mut self, tag: impl AsRef<str>) {
        let tag = tag.as_ref().trim().to_lowercase();
        if !tag.is_empty() && !self.tags.contains(&tag) {
            self.tags.push(tag);
        }
    }

    pub fn has_tag(&self, tag: &str) -> bool {
        let tag = tag.trim().to_lowercase();
        self.tags.contains(&tag)
    }
}

/// How urgent a task is, independent of its tags.
///
/// `None` is the default and sorts last — an unprioritized task is not the same
/// as a low-priority one, which the user chose to deprioritize.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
pub enum Priority {
    High,
    Medium,
    Low,
    #[default]
    None,
}

impl Priority {
    /// Parses the forms a user might type: "p1", "high", "!!!".
    pub fn parse(word: &str) -> Option<Self> {
        Some(match word.trim().to_lowercase().as_str() {
            "p1" | "1" | "high" | "urgent" | "!!!" => Priority::High,
            "p2" | "2" | "medium" | "med" | "!!" => Priority::Medium,
            "p3" | "3" | "low" | "!" => Priority::Low,
            "p4" | "4" | "none" => Priority::None,
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            Priority::High => "high",
            Priority::Medium => "medium",
            Priority::Low => "low",
            Priority::None => "none",
        }
    }
}

/// A checklist item inside a task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subtask {
    pub id: Uuid,
    pub title: String,
    pub done: bool,
}

impl Subtask {
    pub fn new(title: impl Into<String>) -> Self {
        Self { id: Uuid::new_v4(), title: title.into(), done: false }
    }
}

/// How a task's subtask list is meant to be read.
///
/// The second mode is the one that makes a standing task useful: "Learning"
/// repeats every week and holds a list of things to learn, and each week you
/// tick one off. The occurrence is done because an item was done — the rest of
/// the list is simply what is left, not work that is outstanding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Checklist {
    /// Every item is part of this one task. Completing the task is up to you.
    #[default]
    All,
    /// One item per occurrence. Ticking an item finishes the occurrence and
    /// moves the series on.
    OnePerOccurrence,
}

impl Checklist {
    pub fn label(self) -> &'static str {
        match self {
            Checklist::All => "all",
            Checklist::OnePerOccurrence => "one-per-occurrence",
        }
    }

    pub fn parse(text: &str) -> Self {
        match text {
            "one-per-occurrence" => Checklist::OnePerOccurrence,
            _ => Checklist::All,
        }
    }
}

impl Task {
    /// The items still waiting, for a task whose list is a backlog.
    pub fn remaining(&self) -> impl Iterator<Item = &Subtask> {
        self.subtasks.iter().filter(|s| !s.done)
    }
}

/// A set of weekdays, stored as a bitmask so [`Recurrence`] stays `Copy`.
///
/// An empty set on a weekly rule means "whatever weekday the series is anchored
/// to" — "every 2 weeks" repeats on the anchor's own day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct WeekdaySet(u8);

impl WeekdaySet {
    pub const EMPTY: Self = Self(0);
    /// Monday through Friday.
    pub const WEEKDAYS: Self = Self(0b0001_1111);
    /// Saturday and Sunday.
    pub const WEEKENDS: Self = Self(0b0110_0000);

    pub fn new() -> Self {
        Self::EMPTY
    }

    pub fn from_day(day: Weekday) -> Self {
        Self::EMPTY.with(day)
    }

    pub fn with(self, day: Weekday) -> Self {
        Self(self.0 | (1 << day.num_days_from_monday()))
    }

    pub fn insert(&mut self, day: Weekday) {
        *self = self.with(day);
    }

    /// Every day in either set.
    pub fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub fn contains(self, day: Weekday) -> bool {
        self.0 & (1 << day.num_days_from_monday()) != 0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn len(self) -> u32 {
        self.0.count_ones()
    }

    /// The days in the set, Monday first.
    pub fn days(self) -> impl Iterator<Item = Weekday> {
        const ORDER: [Weekday; 7] = [
            Weekday::Mon,
            Weekday::Tue,
            Weekday::Wed,
            Weekday::Thu,
            Weekday::Fri,
            Weekday::Sat,
            Weekday::Sun,
        ];
        ORDER.into_iter().filter(move |d| self.contains(*d))
    }
}

impl FromIterator<Weekday> for WeekdaySet {
    fn from_iter<I: IntoIterator<Item = Weekday>>(iter: I) -> Self {
        iter.into_iter().fold(Self::EMPTY, |set, day| set.with(day))
    }
}

/// How a task repeats.
///
/// Deliberately narrower than RFC 5545 — it covers the phrases the parser
/// recognizes and nothing more. "Every weekday" and "every weekend" are not
/// separate variants: they are [`Weekly`](Recurrence::Weekly) over the
/// corresponding day set, which keeps the expansion engine to one weekly path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Recurrence {
    /// "every day", "daily"
    Daily,
    /// "every 3 days", "every other day"
    EveryNDays(u32),
    /// "every week", "every monday", "mondays", "every monday and wednesday"
    Weekly { days: WeekdaySet },
    /// "every other week", "every 3 weeks", "every other tuesday"
    EveryNWeeks { n: u32, days: WeekdaySet },
    /// "every month", "every 15th"
    Monthly { day: Option<u32> },
    /// "every 3 months", "every other month"
    EveryNMonths(u32),
    /// "every year", "annually"
    Yearly,
}

impl Recurrence {
    /// "every weekday" — Monday through Friday.
    pub const WEEKDAYS: Self = Recurrence::Weekly { days: WeekdaySet::WEEKDAYS };
    /// "every weekend" — Saturday and Sunday.
    pub const WEEKENDS: Self = Recurrence::Weekly { days: WeekdaySet::WEEKENDS };

    /// A human phrase for the rule, for list rows and tooltips. Lives here
    /// rather than in a client so every shell says the same thing.
    pub fn describe(self) -> String {
        match self {
            Recurrence::Daily => "every day".into(),
            Recurrence::EveryNDays(2) => "every other day".into(),
            Recurrence::EveryNDays(n) => format!("every {n} days"),
            Recurrence::Weekly { days } => match days {
                d if d.is_empty() => "every week".into(),
                d if d == WeekdaySet::WEEKDAYS => "every weekday".into(),
                d if d == WeekdaySet::WEEKENDS => "every weekend".into(),
                d => format!("every {}", join_days(d)),
            },
            Recurrence::EveryNWeeks { n, days } => {
                let every = if n == 2 {
                    "every other week".to_string()
                } else {
                    format!("every {n} weeks")
                };
                if days.is_empty() {
                    every
                } else {
                    format!("{every} on {}", join_days(days))
                }
            }
            Recurrence::Monthly { day: None } => "every month".into(),
            Recurrence::Monthly { day: Some(d) } => format!("every month on the {}", ordinal(d)),
            Recurrence::EveryNMonths(2) => "every other month".into(),
            Recurrence::EveryNMonths(n) => format!("every {n} months"),
            Recurrence::Yearly => "every year".into(),
        }
    }

    /// Collapses the degenerate "every 1 X" forms onto their plain equivalents,
    /// so callers never have to treat `EveryNDays(1)` and `Daily` separately.
    pub(crate) fn normalized(self) -> Self {
        match self {
            Recurrence::EveryNDays(1) => Recurrence::Daily,
            Recurrence::EveryNWeeks { n: 1, days } => Recurrence::Weekly { days },
            Recurrence::EveryNMonths(1) => Recurrence::Monthly { day: None },
            other => other,
        }
    }
}

/// A single occurrence that departs from the series, so one moved or skipped
/// instance never rewrites the rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exception {
    /// The date the rule would have produced.
    pub date: NaiveDate,
    pub action: ExceptionAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExceptionAction {
    /// This occurrence does not happen; the series continues.
    Skip,
    /// This occurrence happens on a different date instead.
    MoveTo(NaiveDate),
}

impl Exception {
    pub fn skip(date: NaiveDate) -> Self {
        Self { date, action: ExceptionAction::Skip }
    }

    pub fn move_to(date: NaiveDate, to: NaiveDate) -> Self {
        Self { date, action: ExceptionAction::MoveTo(to) }
    }
}

/// "Monday and Wednesday", "Monday, Wednesday and Friday".
fn join_days(days: WeekdaySet) -> String {
    let names: Vec<String> = days.days().map(|d| day_name(d).to_string()).collect();
    match names.split_last() {
        None => String::new(),
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
    }
}

fn day_name(day: Weekday) -> &'static str {
    match day {
        Weekday::Mon => "Monday",
        Weekday::Tue => "Tuesday",
        Weekday::Wed => "Wednesday",
        Weekday::Thu => "Thursday",
        Weekday::Fri => "Friday",
        Weekday::Sat => "Saturday",
        Weekday::Sun => "Sunday",
    }
}

/// 1 -> "1st", 22 -> "22nd". The teens are all "th".
fn ordinal(n: u32) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}
