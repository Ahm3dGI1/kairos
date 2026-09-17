//! Expansion of recurrence rules into dates, and what completing one does.
//!
//! The anchor throughout is Sunday 2026-09-13 — a Sunday, so a rule naming a
//! weekday has to move forward off the anchor to find its first occurrence.

use chrono::{NaiveDate, Weekday};
use mtodo_core::{
    complete_occurrence, next_occurrence, occurrences, Exception, Recurrence, Task, WeekdaySet,
};

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

const ANCHOR: (i32, u32, u32) = (2026, 9, 13);

fn anchor() -> NaiveDate {
    d(ANCHOR.0, ANCHOR.1, ANCHOR.2)
}

/// A recurring task anchored at 2026-09-13.
fn task(rule: Recurrence) -> Task {
    let mut task = Task::new("test", anchor());
    task.due = Some(anchor());
    task.recurrence = Some(rule);
    task
}

fn weekly(days: &[Weekday]) -> Recurrence {
    Recurrence::Weekly { days: days.iter().copied().collect() }
}

/// Occurrences within a window starting at the anchor.
fn within(task: &Task, days: i64) -> Vec<NaiveDate> {
    occurrences(task, anchor(), anchor() + chrono::Duration::days(days))
}

#[test]
fn daily() {
    let dates = within(&task(Recurrence::Daily), 3);
    assert_eq!(dates, [d(2026, 9, 13), d(2026, 9, 14), d(2026, 9, 15), d(2026, 9, 16)]);
}

#[test]
fn every_n_days() {
    let dates = within(&task(Recurrence::EveryNDays(3)), 9);
    assert_eq!(dates, [d(2026, 9, 13), d(2026, 9, 16), d(2026, 9, 19), d(2026, 9, 22)]);
}

#[test]
fn weekly_on_one_day() {
    // Anchored on a Sunday, "every monday" starts the next day.
    let dates = within(&task(weekly(&[Weekday::Mon])), 22);
    assert_eq!(dates, [d(2026, 9, 14), d(2026, 9, 21), d(2026, 9, 28), d(2026, 10, 5)]);
}

#[test]
fn weekly_with_no_day_named_follows_the_anchor() {
    // "every week" repeats on whatever day the series started.
    let dates = within(&task(Recurrence::Weekly { days: WeekdaySet::EMPTY }), 21);
    assert_eq!(dates, [d(2026, 9, 13), d(2026, 9, 20), d(2026, 9, 27), d(2026, 10, 4)]);
}

#[test]
fn weekly_on_several_days() {
    let dates = within(&task(weekly(&[Weekday::Mon, Weekday::Wed, Weekday::Fri])), 14);
    assert_eq!(
        dates,
        [
            d(2026, 9, 14),
            d(2026, 9, 16),
            d(2026, 9, 18),
            d(2026, 9, 21),
            d(2026, 9, 23),
            d(2026, 9, 25),
        ]
    );
}

#[test]
fn every_other_week_starts_at_the_first_matching_day() {
    // The regression this guards: blocks used to count from the anchor's own
    // week, so a Sunday anchor skipped the coming Friday and started a week
    // late. "Every other friday" must begin with Sep 18, not Sep 25.
    let rule = Recurrence::EveryNWeeks { n: 2, days: WeekdaySet::from_day(Weekday::Fri) };
    let dates = within(&task(rule), 47);
    assert_eq!(dates, [d(2026, 9, 18), d(2026, 10, 2), d(2026, 10, 16), d(2026, 10, 30)]);
}

#[test]
fn weekdays_and_weekends() {
    let weekdays = within(&task(Recurrence::WEEKDAYS), 7);
    assert_eq!(
        weekdays,
        [d(2026, 9, 14), d(2026, 9, 15), d(2026, 9, 16), d(2026, 9, 17), d(2026, 9, 18)]
    );

    let weekends = within(&task(Recurrence::WEEKENDS), 8);
    assert_eq!(weekends, [d(2026, 9, 13), d(2026, 9, 19), d(2026, 9, 20)]);
}

#[test]
fn monthly() {
    let same_day = within(&task(Recurrence::Monthly { day: None }), 91);
    assert_eq!(same_day, [d(2026, 9, 13), d(2026, 10, 13), d(2026, 11, 13), d(2026, 12, 13)]);

    // A named day earlier in the month starts next month.
    let first = within(&task(Recurrence::Monthly { day: Some(1) }), 90);
    assert_eq!(first, [d(2026, 10, 1), d(2026, 11, 1), d(2026, 12, 1)]);
}

#[test]
fn a_month_day_that_does_not_exist_clamps_rather_than_skipping() {
    let mut task = task(Recurrence::Monthly { day: Some(31) });
    task.due = Some(d(2027, 1, 31));
    let dates = occurrences(&task, d(2027, 1, 1), d(2027, 4, 30));
    // February has no 31st, so the occurrence lands on the 28th instead of
    // vanishing from the series.
    assert_eq!(dates, [d(2027, 1, 31), d(2027, 2, 28), d(2027, 3, 31), d(2027, 4, 30)]);
}

#[test]
fn every_n_months_and_yearly() {
    let quarterly = within(&task(Recurrence::EveryNMonths(3)), 400);
    assert_eq!(
        quarterly,
        [d(2026, 9, 13), d(2026, 12, 13), d(2027, 3, 13), d(2027, 6, 13), d(2027, 9, 13)]
    );

    let yearly = within(&task(Recurrence::Yearly), 800);
    assert_eq!(yearly, [d(2026, 9, 13), d(2027, 9, 13), d(2028, 9, 13)]);
}

#[test]
fn a_skipped_occurrence_leaves_the_series_intact() {
    let mut task = task(Recurrence::Daily);
    task.exceptions.push(Exception::skip(d(2026, 9, 14)));

    let dates = within(&task, 3);
    assert_eq!(dates, [d(2026, 9, 13), d(2026, 9, 15), d(2026, 9, 16)]);
}

#[test]
fn a_moved_occurrence_happens_on_its_new_date() {
    let mut task = task(Recurrence::Daily);
    task.exceptions.push(Exception::move_to(d(2026, 9, 14), d(2026, 9, 17)));

    let dates = within(&task, 4);
    // The 14th is gone and the 17th now carries two occurrences, deduplicated.
    assert_eq!(dates, [d(2026, 9, 13), d(2026, 9, 15), d(2026, 9, 16), d(2026, 9, 17)]);
}

#[test]
fn an_occurrence_moved_backwards_into_the_window_is_still_found() {
    let mut task = task(weekly(&[Weekday::Mon]));
    // Pull the October 5th occurrence back to September 15th.
    task.exceptions.push(Exception::move_to(d(2026, 10, 5), d(2026, 9, 15)));

    let dates = within(&task, 10);
    assert!(dates.contains(&d(2026, 9, 15)), "moved occurrence missing from {dates:?}");
}

#[test]
fn next_occurrence_respects_exceptions() {
    let mut task = task(Recurrence::Daily);
    task.exceptions.push(Exception::skip(d(2026, 9, 14)));

    assert_eq!(next_occurrence(&task, d(2026, 9, 13)), Some(d(2026, 9, 13)));
    assert_eq!(next_occurrence(&task, d(2026, 9, 14)), Some(d(2026, 9, 15)));
}

#[test]
fn a_one_off_task_has_one_occurrence() {
    let mut task = Task::new("buy milk", anchor());
    task.due = Some(d(2026, 9, 20));

    assert_eq!(occurrences(&task, d(2026, 9, 1), d(2026, 9, 30)), [d(2026, 9, 20)]);
    assert!(occurrences(&task, d(2026, 10, 1), d(2026, 10, 31)).is_empty());
    assert_eq!(next_occurrence(&task, anchor()), Some(d(2026, 9, 20)));
}

#[test]
fn a_task_with_no_due_date_never_occurs() {
    let task = Task::new("someday", anchor());
    assert!(occurrences(&task, d(2020, 1, 1), d(2030, 1, 1)).is_empty());
    assert_eq!(next_occurrence(&task, anchor()), None);
}

#[test]
fn completing_a_recurring_task_advances_it_instead_of_closing_it() {
    let mut task = task(Recurrence::Daily);

    let next = complete_occurrence(&mut task, anchor());
    assert_eq!(next, Some(d(2026, 9, 14)));
    assert_eq!(task.due, Some(d(2026, 9, 14)));
    assert!(!task.is_done(), "the series should still be open");
}

#[test]
fn completing_a_one_off_task_closes_it() {
    let mut task = Task::new("buy milk", anchor());
    task.due = Some(anchor());

    assert_eq!(complete_occurrence(&mut task, anchor()), None);
    assert_eq!(task.completed_at, Some(anchor()));
    assert!(task.is_done());
}

#[test]
fn completing_ahead_of_schedule_does_not_rewind_the_series() {
    let mut task = task(weekly(&[Weekday::Mon]));
    task.due = Some(d(2026, 9, 21));

    // Ticking it off on the 14th should still leave the 28th next, not pull
    // the series back to the 21st it had already moved past.
    complete_occurrence(&mut task, d(2026, 9, 14));
    assert_eq!(task.due, Some(d(2026, 9, 28)));
}
