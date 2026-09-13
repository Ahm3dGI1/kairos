//! The task model. Every client speaks in these types; nothing here knows about
//! UI, storage, or platform APIs.

use chrono::{NaiveDate, NaiveTime, Weekday};

/// A task as captured from a single line of input.
///
/// Every field except `title` is optional: "buy milk" is a valid task with no
/// date, time, or recurrence. Fields are public and mutable so a client can let
/// the user override anything the parser inferred — see
/// [`ParseResult`](crate::ParseResult) for what was inferred and how confidently.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Task {
    /// The input line with every recognized date/time/recurrence phrase removed.
    pub title: String,
    /// Local calendar date. Naive by design: the shell supplies the timezone.
    pub date: Option<NaiveDate>,
    /// Local wall-clock time.
    pub time: Option<NaiveTime>,
    pub recurrence: Option<Recurrence>,
}

impl Task {
    pub fn new(title: impl Into<String>) -> Self {
        Self { title: title.into(), ..Default::default() }
    }
}

/// How a task repeats.
///
/// Deliberately narrower than RFC 5545 — it covers the phrases the parser
/// recognizes and nothing more. When occurrence expansion lands (and with it
/// per-occurrence exceptions), this becomes the input to an `rrule`-backed
/// generator rather than growing more variants of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recurrence {
    /// "every day", "daily"
    Daily,
    /// "every 3 days", "every other day"
    EveryNDays(u32),
    /// "every week", "every monday", "mondays"
    Weekly { weekday: Option<Weekday> },
    /// "every other week", "every 3 weeks", "every other tuesday"
    EveryNWeeks { n: u32, weekday: Option<Weekday> },
    /// "every weekday" — Monday through Friday
    Weekdays,
    /// "every weekend" — Saturday and Sunday
    Weekends,
    /// "every month", "every 15th"
    Monthly { day: Option<u32> },
    /// "every 3 months", "every other month"
    EveryNMonths(u32),
    /// "every year", "annually"
    Yearly,
}

impl Recurrence {
    /// Collapses the degenerate "every 1 X" forms onto their plain equivalents,
    /// so callers never have to treat `EveryNDays(1)` and `Daily` separately.
    pub(crate) fn normalized(self) -> Self {
        match self {
            Recurrence::EveryNDays(1) => Recurrence::Daily,
            Recurrence::EveryNWeeks { n: 1, weekday } => Recurrence::Weekly { weekday },
            Recurrence::EveryNMonths(1) => Recurrence::Monthly { day: None },
            other => other,
        }
    }
}
