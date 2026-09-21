//! The agenda grouping, and the habits/journal day log.

use std::collections::HashSet;

use chrono::NaiveDate;
use kairos_core::agenda::{self, Bucket};
use kairos_core::daily::{self, DayLog};
use kairos_core::{JournalEntry, MonthJournal, Recurrence, Store, Task};

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

fn today() -> NaiveDate {
    d(2026, 9, 18)
}

/// An open task due on `due`.
fn task(title: &str, due: Option<NaiveDate>) -> Task {
    let mut task = Task::new(title, today());
    task.due = due;
    task
}

#[test]
fn every_task_lands_in_the_right_section() {
    let cases = [
        (task("last week", Some(d(2026, 9, 11))), Bucket::Overdue),
        (task("yesterday", Some(d(2026, 9, 17))), Bucket::Overdue),
        (task("today", Some(today())), Bucket::Today),
        (task("tomorrow", Some(d(2026, 9, 19))), Bucket::Tomorrow),
        (task("in three days", Some(d(2026, 9, 21))), Bucket::ThisWeek),
        (task("on the seventh day", Some(d(2026, 9, 25))), Bucket::ThisWeek),
        (task("past the week", Some(d(2026, 9, 26))), Bucket::Later),
        (task("no date at all", None), Bucket::Someday),
    ];
    for (task, expected) in cases {
        assert_eq!(Bucket::of(&task, today()), expected, "for {:?}", task.title);
    }
}

#[test]
fn a_completed_task_is_completed_whatever_its_date() {
    let mut overdue = task("was late", Some(d(2026, 1, 1)));
    overdue.completed_at = Some(today());
    assert_eq!(Bucket::of(&overdue, today()), Bucket::Completed);
}

#[test]
fn a_recurring_task_is_placed_by_its_next_occurrence() {
    let mut weekly = task("standup", Some(today()));
    weekly.recurrence =
        Some(Recurrence::Weekly { days: kairos_core::WeekdaySet::from_day(chrono::Weekday::Mon) });
    // Today is a Friday; the next Monday is three days out.
    assert_eq!(Bucket::of(&weekly, today()), Bucket::ThisWeek);
}

#[test]
fn a_recurring_task_left_undone_is_still_overdue() {
    // The rule would happily offer today, but the anchor says it was missed —
    // a daily habit-shaped task should not quietly forgive a missed day.
    let mut daily_task = task("water plants", Some(d(2026, 9, 15)));
    daily_task.recurrence = Some(Recurrence::Daily);
    assert_eq!(Bucket::of(&daily_task, today()), Bucket::Overdue);
}

#[test]
fn grouping_drops_empty_sections_and_orders_within_them() {
    let tasks = [
        task("later", Some(d(2026, 10, 20))),
        task("today second", Some(today())),
        task("overdue", Some(d(2026, 9, 1))),
        task("today first", Some(today())),
    ];
    let mut first = tasks[3].clone();
    first.priority = kairos_core::Priority::High;

    let groups = agenda::group(
        vec![tasks[0].clone(), tasks[1].clone(), tasks[2].clone(), first],
        today(),
        false,
    );

    let buckets: Vec<Bucket> = groups.iter().map(|g| g.bucket).collect();
    assert_eq!(buckets, [Bucket::Overdue, Bucket::Today, Bucket::Later]);

    // Same date, so priority breaks the tie.
    let today_titles: Vec<&str> = groups[1].tasks.iter().map(|t| t.title.as_str()).collect();
    assert_eq!(today_titles, ["today first", "today second"]);
}

#[test]
fn completed_work_appears_only_when_asked_for() {
    let mut done = task("finished", Some(today()));
    done.completed_at = Some(today());
    let tasks = vec![done, task("open", Some(today()))];

    let without = agenda::group(tasks.clone(), today(), false);
    assert_eq!(without.iter().map(|g| g.bucket).collect::<Vec<_>>(), [Bucket::Today]);

    let with = agenda::group(tasks, today(), true);
    assert_eq!(
        with.iter().map(|g| g.bucket).collect::<Vec<_>>(),
        [Bucket::Today, Bucket::Completed]
    );
}

// ---------- habits and the day log ----------

#[test]
fn a_streak_counts_back_from_today() {
    let done: HashSet<NaiveDate> =
        [d(2026, 9, 16), d(2026, 9, 17), d(2026, 9, 18)].into_iter().collect();
    assert_eq!(daily::streak(&done, today()), 3);
}

#[test]
fn today_being_untouched_does_not_break_the_streak() {
    // The day is not over. A tracker that zeroes your run every morning is one
    // you stop trusting.
    let done: HashSet<NaiveDate> = [d(2026, 9, 16), d(2026, 9, 17)].into_iter().collect();
    assert_eq!(daily::streak(&done, today()), 2);

    // But a gap before yesterday does break it.
    let gap: HashSet<NaiveDate> = [d(2026, 9, 14), d(2026, 9, 15)].into_iter().collect();
    assert_eq!(daily::streak(&gap, today()), 0);
}

#[test]
fn durations_accept_the_shapes_a_person_types() {
    let cases = [
        ("90", 90),
        ("90m", 90),
        ("7h", 420),
        ("7h30", 450),
        ("7h 30m", 450),
        ("7.5h", 450),
        ("7,5h", 450),
    ];
    for (text, expected) in cases {
        assert_eq!(daily::parse_duration(text), Some(expected), "for {text:?}");
    }
    assert_eq!(daily::parse_duration(""), None);
    assert_eq!(daily::parse_duration("nonsense"), None);
}

#[test]
fn durations_read_back_the_way_they_are_written() {
    assert_eq!(daily::format_duration(450), "7h 30m");
    assert_eq!(daily::format_duration(420), "7h");
    assert_eq!(daily::format_duration(45), "45m");
}

#[test]
fn habits_and_a_day_log_round_trip() {
    let mut store = Store::in_memory().unwrap();

    let gym = store.add_habit("gym", today()).unwrap();
    let read = store.add_habit("read", today()).unwrap();
    assert_eq!(store.habits(false).unwrap().len(), 2);
    // Position keeps insertion order.
    assert_eq!(store.habits(false).unwrap()[0].name, "gym");

    assert!(store.toggle_habit(gym.id, today()).unwrap());
    let log = store.day_log(today()).unwrap();
    assert!(log.is_done(gym.id));
    assert!(!log.is_done(read.id));

    // Toggling again clears it.
    assert!(!store.toggle_habit(gym.id, today()).unwrap());
    assert!(!store.day_log(today()).unwrap().is_done(gym.id));
}

#[test]
fn the_journal_and_a_days_numbers_persist() {
    let mut store = Store::in_memory().unwrap();

    let mut sleep = store.add_habit("Sleep", today()).unwrap();
    sleep.kind = kairos_core::daily::HabitKind::Duration;
    store.save_habit(&sleep).unwrap();
    let mut pages = store.add_habit("Pages", today()).unwrap();
    pages.kind = kairos_core::daily::HabitKind::Number { unit: "pages".into() };
    store.save_habit(&pages).unwrap();

    let mut log = DayLog::new(today());
    log.journal = "Slept badly, shipped the parser anyway.".into();
    log.set_value(sleep.id, Some(390.0));
    log.set_value(pages.id, Some(12.0));
    store.save_day_log(&log).unwrap();

    let loaded = store.day_log(today()).unwrap();
    assert_eq!(loaded, log);
}

/// An emptied cell is a day with nothing recorded, not a day with a zero —
/// which is the difference between "I did not measure" and "I slept none".
#[test]
fn clearing_a_number_removes_it_rather_than_zeroing_it() {
    let mut store = Store::in_memory().unwrap();
    let habit = store.add_habit("Pages", today()).unwrap();

    let mut log = DayLog::new(today());
    log.set_value(habit.id, Some(12.0));
    store.save_day_log(&log).unwrap();
    assert_eq!(store.day_log(today()).unwrap().value(habit.id), Some(12.0));

    let mut log = store.day_log(today()).unwrap();
    log.set_value(habit.id, None);
    store.save_day_log(&log).unwrap();

    let loaded = store.day_log(today()).unwrap();
    assert_eq!(loaded.value(habit.id), None);
    assert!(loaded.is_empty(), "a day with nothing on it keeps no row");
}

#[test]
fn an_emptied_day_stops_taking_up_a_row() {
    let mut store = Store::in_memory().unwrap();

    let mut log = DayLog::new(today());
    log.journal = "something".into();
    store.save_day_log(&log).unwrap();
    assert_eq!(store.day_logs_between(today(), today()).unwrap().len(), 1);

    log.journal = "   ".into();
    store.save_day_log(&log).unwrap();
    assert!(store.day_logs_between(today(), today()).unwrap().is_empty());
}

#[test]
fn streaks_come_back_per_habit() {
    let mut store = Store::in_memory().unwrap();
    let gym = store.add_habit("gym", today()).unwrap();
    let read = store.add_habit("read", today()).unwrap();

    for day in [d(2026, 9, 16), d(2026, 9, 17), d(2026, 9, 18)] {
        store.toggle_habit(gym.id, day).unwrap();
    }
    store.toggle_habit(read.id, d(2026, 9, 18)).unwrap();

    let streaks = store.habit_streaks(today()).unwrap();
    assert_eq!(streaks.get(&gym.id), Some(&3));
    assert_eq!(streaks.get(&read.id), Some(&1));
}

#[test]
fn deleting_a_habit_takes_its_ticks_with_it() {
    let mut store = Store::in_memory().unwrap();
    let gym = store.add_habit("gym", today()).unwrap();
    store.toggle_habit(gym.id, today()).unwrap();

    store.delete_habit(gym.id).unwrap();

    assert!(store.habits(true).unwrap().is_empty());
    // The tick lived inside the day's row, so it has to have been pruned.
    assert!(!store.day_log(today()).unwrap().is_done(gym.id));
}

#[test]
fn archiving_keeps_a_habit_out_of_the_list_without_losing_it() {
    let mut store = Store::in_memory().unwrap();
    let mut gym = store.add_habit("gym", today()).unwrap();

    gym.archived = true;
    store.save_habit(&gym).unwrap();

    assert!(store.habits(false).unwrap().is_empty());
    assert_eq!(store.habits(true).unwrap().len(), 1);
}

// ---------- the month journal ----------

#[test]
fn a_month_journal_round_trips() {
    let mut store = Store::in_memory().unwrap();

    let mut journal = MonthJournal::new(2026, 9);
    let mut first = JournalEntry::new("Friday 18");
    first.body = "Shipped the agenda rewrite.".into();
    let mut second = JournalEntry::new("The trip");
    second.body = "Booked flights.".into();
    journal.entries = vec![first, second];

    store.save_month_journal(&journal).unwrap();
    assert_eq!(store.month_journal(2026, 9).unwrap(), journal);

    // A different month is a different page.
    assert!(store.month_journal(2026, 10).unwrap().entries.is_empty());
}

#[test]
fn an_emptied_month_journal_stops_taking_up_a_row() {
    let mut store = Store::in_memory().unwrap();

    let mut journal = MonthJournal::new(2026, 9);
    journal.entries.push(JournalEntry::new("something"));
    store.save_month_journal(&journal).unwrap();
    assert_eq!(store.month_journal(2026, 9).unwrap().entries.len(), 1);

    journal.entries = vec![JournalEntry::new("   ")];
    store.save_month_journal(&journal).unwrap();
    assert!(store.month_journal(2026, 9).unwrap().entries.is_empty());
}

#[test]
fn pruning_drops_only_the_blank_entries() {
    let mut journal = MonthJournal::new(2026, 9);
    let mut kept = JournalEntry::new("");
    kept.body = "no heading, still worth keeping".into();
    journal.entries = vec![JournalEntry::new("  "), kept.clone()];

    journal.prune();
    assert_eq!(journal.entries, vec![kept]);
}
