use chrono::{Datelike, Duration, NaiveDate, Weekday};

use crate::task::{Checklist, Exception, ExceptionAction, Recurrence, Task, WeekdaySet};

/// How far the expander will search before giving up, in raw occurrences.
/// Bounds the pathological case where every candidate is excepted away.
const MAX_STEPS: usize = 4096;

/// The dates a task falls on within `from..=to`, exceptions applied.
pub fn occurrences(task: &Task, from: NaiveDate, to: NaiveDate) -> Vec<NaiveDate> {
    let Some(anchor) = task.due else { return Vec::new() };
    let Some(rule) = task.recurrence else {
        return if (from..=to).contains(&anchor) { vec![anchor] } else { Vec::new() };
    };

    let mut dates = Vec::new();
    for raw in RawOccurrences::new(task.recurrence_anchor.unwrap_or(anchor), rule).take(MAX_STEPS) {
        // Occurrences are generated in order, so once the raw date passes the
        // window we are done — except that a moved one can land back inside it,
        // which is why the walk continues a bounded distance past `to`.
        if raw > to.checked_add_signed(Duration::days(LOOKAHEAD_DAYS)).unwrap_or(NaiveDate::MAX) {
            break;
        }
        if let Some(actual) = apply_exceptions(&task.exceptions, raw) {
            if actual >= anchor && (from..=to).contains(&actual) {
                dates.push(actual);
            }
        }
    }
    dates.sort_unstable();
    dates.dedup();
    dates
}

/// How far past the requested window to keep walking, so an occurrence moved
/// backwards into the window is still found.
const LOOKAHEAD_DAYS: i64 = 400;

/// The first occurrence on or after `on`, exceptions applied.
pub fn next_occurrence(task: &Task, on: NaiveDate) -> Option<NaiveDate> {
    let anchor = task.due?;
    let Some(rule) = task.recurrence else {
        return (anchor >= on).then_some(anchor);
    };

    let mut best: Option<NaiveDate> = None;
    for raw in RawOccurrences::new(task.recurrence_anchor.unwrap_or(anchor), rule).take(MAX_STEPS) {
        if let Some(actual) = apply_exceptions(&task.exceptions, raw) {
            if actual >= on.max(anchor) && best.is_none_or(|b| actual < b) {
                best = Some(actual);
            }
        }
        // A raw date this far past the best candidate cannot be moved back
        // before it, so nothing better remains.
        if let Some(b) = best {
            if raw > b.checked_add_signed(Duration::days(LOOKAHEAD_DAYS)).unwrap_or(NaiveDate::MAX)
            {
                break;
            }
        }
    }
    best
}

/// Resolves one raw date against the exception list: `None` when skipped,
/// otherwise the date it actually happens on.
fn apply_exceptions(exceptions: &[Exception], raw: NaiveDate) -> Option<NaiveDate> {
    match exceptions.iter().find(|e| e.date == raw) {
        Some(Exception { action: ExceptionAction::Skip, .. }) => None,
        Some(Exception { action: ExceptionAction::MoveTo(to), .. }) => Some(*to),
        None => Some(raw),
    }
}

/// Completes one occurrence of a task.
pub fn complete_occurrence(task: &mut Task, on: NaiveDate) -> Option<NaiveDate> {
    if !task.is_recurring() {
        task.completed_at = Some(on);
        return None;
    }
    task.recurrence_anchor = task.recurrence_anchor.or(task.due);
    let after = task.due.map_or(on, |due| due.max(on));
    let next = next_occurrence(task, after.succ_opt()?);
    // A series that has run out completes for good.
    match next {
        Some(date) => {
            task.due = Some(date);
            Some(date)
        }
        None => {
            task.completed_at = Some(on);
            None
        }
    }
}

/// Raw rule dates from the anchor forward, before exceptions.
struct RawOccurrences {
    anchor: NaiveDate,
    rule: Recurrence,
    /// Monday of the week the series actually starts in. Weekly blocks count
    /// from here rather than from the anchor's own week, so "every other
    /// friday" entered on a Sunday starts with the coming Friday instead of
    /// skipping it to the one after.
    base_monday: NaiveDate,
    /// Which step of the rule to emit next.
    step: u32,
    /// Position within a weekly block's day set.
    day_index: usize,
    done: bool,
}

impl RawOccurrences {
    fn new(anchor: NaiveDate, rule: Recurrence) -> Self {
        let base_monday = match rule {
            Recurrence::Weekly { days } | Recurrence::EveryNWeeks { days, .. } => {
                let set =
                    if days.is_empty() { WeekdaySet::from_day(anchor.weekday()) } else { days };
                let first = (0..7)
                    .filter_map(|offset| anchor.checked_add_signed(Duration::days(offset)))
                    .find(|date| set.contains(date.weekday()))
                    .unwrap_or(anchor);
                monday_of(first)
            }
            _ => anchor,
        };
        Self { anchor, rule, base_monday, step: 0, day_index: 0, done: false }
    }

    /// The day set a weekly rule repeats on, defaulting to the anchor's own
    /// weekday when the rule names none.
    fn weekly_days(&self, days: WeekdaySet) -> Vec<Weekday> {
        if days.is_empty() {
            vec![self.anchor.weekday()]
        } else {
            days.days().collect()
        }
    }
}

impl Iterator for RawOccurrences {
    type Item = NaiveDate;

    fn next(&mut self) -> Option<NaiveDate> {
        if self.done {
            return None;
        }
        loop {
            let date = match self.rule {
                Recurrence::Daily => self.anchor.checked_add_signed(days(self.step, 1)?),
                Recurrence::EveryNDays(n) => {
                    self.anchor.checked_add_signed(days(self.step, n.max(1))?)
                }
                Recurrence::Weekly { days: set } => self.weekly_step(set, 1),
                Recurrence::EveryNWeeks { n, days: set } => self.weekly_step(set, n.max(1)),
                Recurrence::Monthly { day } => {
                    let target = day.unwrap_or_else(|| self.anchor.day());
                    monthly(self.anchor, self.step, 1, target)
                }
                Recurrence::EveryNMonths(n) => {
                    monthly(self.anchor, self.step, n.max(1), self.anchor.day())
                }
                Recurrence::Yearly => monthly(self.anchor, self.step, 12, self.anchor.day()),
            };

            let Some(date) = date else {
                self.done = true;
                return None;
            };
            self.advance();

            // Weekly sets and explicit month days can put the first candidates
            // before the anchor; the series starts at the anchor.
            if date >= self.anchor {
                return Some(date);
            }
        }
    }
}

impl RawOccurrences {
    /// The `day_index`-th day of the weekly block `step` blocks after the
    /// anchor's own week.
    fn weekly_step(&self, set: WeekdaySet, every: u32) -> Option<NaiveDate> {
        let days_of_week = self.weekly_days(set);
        let weekday = *days_of_week.get(self.day_index)?;
        let block = self
            .base_monday
            .checked_add_signed(Duration::try_weeks(self.step as i64 * every as i64)?)?;
        block.checked_add_signed(Duration::days(weekday.num_days_from_monday() as i64))
    }

    /// Moves to the next candidate: within a weekly block, that is the next day
    /// in the set; otherwise the next step of the rule.
    fn advance(&mut self) {
        let set = match self.rule {
            Recurrence::Weekly { days } | Recurrence::EveryNWeeks { days, .. } => Some(days),
            _ => None,
        };
        match set {
            Some(days) if self.day_index + 1 < self.weekly_days(days).len() => self.day_index += 1,
            Some(_) => {
                self.day_index = 0;
                self.step += 1;
            }
            None => self.step += 1,
        }
    }
}

fn monday_of(date: NaiveDate) -> NaiveDate {
    date - Duration::days(date.weekday().num_days_from_monday() as i64)
}

fn days(step: u32, every: u32) -> Option<Duration> {
    Duration::try_days(step as i64 * every as i64)
}

/// The occurrence `step` periods of `every` months after the anchor, on day
/// `target` — clamped to the length of the month, so a rule on the 31st falls
/// on the 30th in a 30-day month rather than skipping it.
fn monthly(anchor: NaiveDate, step: u32, every: u32, target: u32) -> Option<NaiveDate> {
    let total = (anchor.year() as i64 * 12 + anchor.month0() as i64)
        .checked_add(step as i64 * every as i64)?;
    let year = i32::try_from(total.div_euclid(12)).ok()?;
    let month = total.rem_euclid(12) as u32 + 1;
    NaiveDate::from_ymd_opt(year, month, target.min(days_in_month(year, month)))
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let (next_year, next_month) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
    NaiveDate::from_ymd_opt(next_year, next_month, 1)
        .and_then(|first| first.pred_opt())
        .map_or(28, |last| last.day())
}

/// Ticks one checklist item, and closes the occurrence when the list is a
pub fn complete_item(task: &mut Task, item: uuid::Uuid, on: NaiveDate) -> Option<NaiveDate> {
    let sub = task.subtasks.iter_mut().find(|s| s.id == item)?;
    sub.done = !sub.done;
    let ticked = sub.done;

    // Only a freshly ticked item on a backlog advances anything. Un-ticking is
    // a correction, and must not push the series forward again.
    if !ticked || task.checklist != Checklist::OnePerOccurrence || !task.is_recurring() {
        return None;
    }
    complete_occurrence(task, on)
}
