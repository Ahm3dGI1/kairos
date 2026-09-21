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

use std::collections::{BTreeMap, HashSet};

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type HabitId = Uuid;

/// What a day's entry for a habit looks like.
///
/// Not every habit is a yes or a no. "Did I read" is a tick; "how many pages"
/// and "how long did I sleep" are numbers, and flattening them to a tick
/// throws away the only part worth looking back at. Sleep and screen time
/// were once two hard-coded columns for exactly this reason — they are now
/// just the first two habits of the kinds that already existed in spirit.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum HabitKind {
    /// Done or not.
    #[default]
    Check,
    /// A quantity, in whatever the user counts.
    Number {
        /// "pages", "km", "glasses" — shown after the value, never parsed.
        #[serde(default)]
        unit: String,
    },
    /// A length of time, entered and shown as "7h 30m".
    Duration,
}

impl HabitKind {
    /// Whether a day's entry is a number rather than a tick.
    pub fn is_numeric(&self) -> bool {
        !matches!(self, HabitKind::Check)
    }

    /// The stored form: "check", "duration", or "number:pages".
    pub fn label(&self) -> String {
        match self {
            HabitKind::Check => "check".into(),
            HabitKind::Duration => "duration".into(),
            HabitKind::Number { unit } if unit.is_empty() => "number".into(),
            HabitKind::Number { unit } => format!("number:{unit}"),
        }
    }

    pub fn parse(text: &str) -> Self {
        match text.split_once(':') {
            Some(("number", unit)) => HabitKind::Number { unit: unit.trim().to_string() },
            _ => match text.trim() {
                "duration" => HabitKind::Duration,
                "number" => HabitKind::Number { unit: String::new() },
                _ => HabitKind::Check,
            },
        }
    }

    /// A value as it should read on the page.
    pub fn format(&self, value: f64) -> String {
        match self {
            HabitKind::Duration => format_duration(value.max(0.0).round() as u32),
            HabitKind::Number { unit } => {
                let number = if (value - value.round()).abs() < f64::EPSILON {
                    format!("{}", value.round() as i64)
                } else {
                    format!("{value:.1}")
                };
                if unit.is_empty() {
                    number
                } else {
                    format!("{number} {unit}")
                }
            }
            HabitKind::Check => String::new(),
        }
    }

    /// Reads what the user typed into a cell.
    pub fn parse_value(&self, text: &str) -> Option<f64> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        match self {
            HabitKind::Duration => parse_duration(text).map(f64::from),
            _ => {
                // The unit is written after the number in the vault, so that a
                // column reads as "42 pages" rather than a bare 42. Anything
                // past the number is that unit, or a typo; either way the
                // number is what was meant.
                let number: String = text
                    .replace(',', ".")
                    .chars()
                    .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
                    .collect();
                number.parse::<f64>().ok().filter(|v| v.is_finite())
            }
        }
    }
}

/// Something you want to do most days.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Habit {
    pub id: HabitId,
    pub name: String,
    #[serde(default)]
    pub kind: HabitKind,
    pub created_at: NaiveDate,
    /// Retired habits stop appearing without losing their history.
    pub archived: bool,
    /// Manual ordering in the list.
    pub position: i64,
}

impl Habit {
    pub fn new(name: impl Into<String>, created_at: NaiveDate, position: i64) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            kind: HabitKind::Check,
            created_at,
            archived: false,
            position,
        }
    }
}

/// Everything recorded about one day.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DayLog {
    pub date: NaiveDate,
    pub journal: String,
    /// The habits ticked on this day.
    pub habits_done: Vec<HabitId>,
    /// What the numeric habits recorded. Ordered by id so the day serialises
    /// the same way twice, which is what keeps the vault from churning.
    #[serde(default)]
    pub values: BTreeMap<HabitId, f64>,
}

impl DayLog {
    pub fn new(date: NaiveDate) -> Self {
        Self { date, journal: String::new(), habits_done: Vec::new(), values: BTreeMap::new() }
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
        self.journal.trim().is_empty() && self.habits_done.is_empty() && self.values.is_empty()
    }

    pub fn value(&self, habit: HabitId) -> Option<f64> {
        self.values.get(&habit).copied()
    }

    /// Records a numeric habit's entry, or clears it when `value` is `None`.
    /// An emptied cell is a day with nothing recorded, not a day with a zero.
    pub fn set_value(&mut self, habit: HabitId, value: Option<f64>) {
        match value {
            Some(value) => {
                self.values.insert(habit, value);
            }
            None => {
                self.values.remove(&habit);
            }
        }
    }

    /// Whether this day has anything at all for a habit, of either kind.
    pub fn has(&self, habit: HabitId) -> bool {
        self.is_done(habit) || self.values.contains_key(&habit)
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
