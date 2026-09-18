//! Habits, the two numbers worth tracking by hand, and the day's journal.
//!
//! A habit is not a task. A task is done once and gone; a habit is a thing you
//! want to have done on as many days as possible, and the interesting question
//! is the run of days, not any single one. Modelling habits as recurring tasks
//! would put them in the agenda competing for attention with real work, and
//! would lose the streak the moment one was completed.
//!
//! Screen time and sleep are entered by hand on purpose: reading them
//! automatically would mean a background agent or a vendor API, and the project
//! is offline-first and self-hosted (spec §3).

use std::collections::HashSet;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type HabitId = Uuid;

/// Something you want to do most days.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Habit {
    pub id: HabitId,
    pub name: String,
    pub created_at: NaiveDate,
    /// Retired habits stop appearing without losing their history.
    pub archived: bool,
    /// Manual ordering in the list.
    pub position: i64,
}

impl Habit {
    pub fn new(name: impl Into<String>, created_at: NaiveDate, position: i64) -> Self {
        Self { id: Uuid::new_v4(), name: name.into(), created_at, archived: false, position }
    }
}

/// Everything recorded about one day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DayLog {
    pub date: NaiveDate,
    pub journal: String,
    /// Minutes in front of a screen, as entered.
    pub screen_minutes: Option<u32>,
    /// Minutes slept, as entered.
    pub sleep_minutes: Option<u32>,
    /// The habits ticked on this day.
    pub habits_done: Vec<HabitId>,
}

impl DayLog {
    pub fn new(date: NaiveDate) -> Self {
        Self {
            date,
            journal: String::new(),
            screen_minutes: None,
            sleep_minutes: None,
            habits_done: Vec::new(),
        }
    }

    pub fn is_done(&self, habit: HabitId) -> bool {
        self.habits_done.contains(&habit)
    }

    /// Ticks or un-ticks a habit for this day, returning the new state.
    pub fn toggle(&mut self, habit: HabitId) -> bool {
        match self.habits_done.iter().position(|id| *id == habit) {
            Some(index) => {
                self.habits_done.remove(index);
                false
            }
            None => {
                self.habits_done.push(habit);
                true
            }
        }
    }

    /// Whether this day holds nothing worth storing — so an opened-and-abandoned
    /// day does not litter the database with blank rows.
    pub fn is_empty(&self) -> bool {
        self.journal.trim().is_empty()
            && self.screen_minutes.is_none()
            && self.sleep_minutes.is_none()
            && self.habits_done.is_empty()
    }
}

/// One block of a month's journal: a heading and whatever goes under it.
///
/// The title is usually a day ("18", "Friday") but nothing enforces that — the
/// journal is a month-long page you write down, not a form with one box per
/// date, so a heading can just as well be "the trip" or nothing at all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalEntry {
    pub id: Uuid,
    pub title: String,
    pub body: String,
}

impl JournalEntry {
    pub fn new(title: impl Into<String>) -> Self {
        Self { id: Uuid::new_v4(), title: title.into(), body: String::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.title.trim().is_empty() && self.body.trim().is_empty()
    }
}

/// A month of journal entries, in the order the user put them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonthJournal {
    pub year: i32,
    pub month: u32,
    pub entries: Vec<JournalEntry>,
}

impl MonthJournal {
    pub fn new(year: i32, month: u32) -> Self {
        Self { year, month, entries: Vec::new() }
    }

    /// "2026-09" — how a month is keyed in the database, and sortable as text.
    pub fn key(year: i32, month: u32) -> String {
        format!("{year:04}-{month:02}")
    }

    pub fn is_empty(&self) -> bool {
        self.entries.iter().all(JournalEntry::is_empty)
    }

    /// Drops blank entries, so an added-then-abandoned heading does not linger.
    pub fn prune(&mut self) {
        self.entries.retain(|entry| !entry.is_empty());
    }
}

/// How many consecutive days up to `today` a habit was ticked.
///
/// Today not being ticked yet does not break the streak: the day is not over,
/// and a tracker that shows your run collapsing to zero every morning is a
/// tracker you stop trusting. The count simply starts from yesterday until you
/// tick it.
pub fn streak(done_days: &HashSet<NaiveDate>, today: NaiveDate) -> u32 {
    let mut day = today;
    if !done_days.contains(&day) {
        let Some(previous) = day.pred_opt() else { return 0 };
        day = previous;
    }

    let mut run = 0;
    while done_days.contains(&day) {
        run += 1;
        match day.pred_opt() {
            Some(previous) => day = previous,
            None => break,
        }
    }
    run
}

/// "7h 30m", "45m" — for display next to the number the user typed.
pub fn format_duration(minutes: u32) -> String {
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m}m"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h {m}m"),
    }
}

/// Reads "7h30", "7h 30m", "7.5h", "90m" or a bare number of minutes.
///
/// Accepting several shapes matters more here than anywhere else in the app:
/// this is a number typed once a day, and being told off for the format is
/// exactly the friction the project exists to remove.
pub fn parse_duration(text: &str) -> Option<u32> {
    let text = text.trim().to_lowercase();
    if text.is_empty() {
        return None;
    }

    // "7.5h" / "7,5h"
    if let Some(hours) = text.strip_suffix('h') {
        let hours: f64 = hours.replace(',', ".").trim().parse().ok()?;
        return (hours >= 0.0).then(|| (hours * 60.0).round() as u32);
    }
    if let Some(minutes) = text.strip_suffix('m') {
        // "7h 30m"
        if let Some((hours, rest)) = minutes.split_once('h') {
            let hours: u32 = hours.trim().parse().ok()?;
            let rest: u32 = rest.trim().parse().unwrap_or(0);
            return Some(hours * 60 + rest);
        }
        return minutes.trim().parse().ok();
    }
    // "7h30"
    if let Some((hours, rest)) = text.split_once('h') {
        let hours: u32 = hours.trim().parse().ok()?;
        let rest: u32 = if rest.trim().is_empty() { 0 } else { rest.trim().parse().ok()? };
        return Some(hours * 60 + rest);
    }
    // A bare number is minutes.
    text.parse().ok()
}
