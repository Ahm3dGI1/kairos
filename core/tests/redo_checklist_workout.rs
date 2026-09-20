//! Redo, the one-per-occurrence checklist, and the workout book.

use chrono::NaiveDate;
use kairos_core::workout::{self, SetEntry};
use kairos_core::{complete_item, Checklist, Recurrence, Store, Subtask, Task};

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

fn today() -> NaiveDate {
    d(2026, 9, 19)
}

// ---------- redo ----------

#[test]
fn redo_replays_what_undo_took_back() {
    let mut store = Store::in_memory().unwrap();
    let mut task = Task::new("first", today());
    store.save(&task).unwrap();

    task.title = "second".into();
    store.save(&task).unwrap();

    store.undo().unwrap();
    assert_eq!(store.get(task.id).unwrap().unwrap().title, "first");
    assert!(store.can_redo().unwrap());

    store.redo().unwrap();
    assert_eq!(store.get(task.id).unwrap().unwrap().title, "second");
}

#[test]
fn redo_walks_back_up_the_whole_stack() {
    let mut store = Store::in_memory().unwrap();
    let mut task = Task::new("one", today());
    store.save(&task).unwrap();
    for title in ["two", "three"] {
        task.title = title.into();
        store.save(&task).unwrap();
    }

    store.undo().unwrap();
    store.undo().unwrap();
    assert_eq!(store.get(task.id).unwrap().unwrap().title, "one");

    store.redo().unwrap();
    assert_eq!(store.get(task.id).unwrap().unwrap().title, "two");
    store.redo().unwrap();
    assert_eq!(store.get(task.id).unwrap().unwrap().title, "three");
    assert!(!store.can_redo().unwrap());
}

#[test]
fn a_fresh_edit_discards_the_redo_stack() {
    // Redoing onto a world that has moved on is not the same act, so the
    // branch is dropped rather than silently reapplied somewhere else.
    let mut store = Store::in_memory().unwrap();
    let mut task = Task::new("one", today());
    store.save(&task).unwrap();
    task.title = "two".into();
    store.save(&task).unwrap();

    store.undo().unwrap();
    assert!(store.can_redo().unwrap());

    task.title = "different".into();
    store.save(&task).unwrap();
    assert!(!store.can_redo().unwrap());
}

#[test]
fn redo_restores_a_deletion() {
    let mut store = Store::in_memory().unwrap();
    let task = Task::new("gone", today());
    store.save(&task).unwrap();
    store.delete(task.id).unwrap();

    store.undo().unwrap();
    assert!(store.get(task.id).unwrap().is_some());

    store.redo().unwrap();
    assert!(store.get(task.id).unwrap().is_none());
}

// ---------- the one-per-occurrence checklist ----------

/// "Learning", repeating weekly, holding the things to get to.
fn learning() -> Task {
    let mut task = Task::new("Learning", today());
    task.due = Some(today());
    task.recurrence = Some(Recurrence::Weekly { days: kairos_core::WeekdaySet::EMPTY });
    task.checklist = Checklist::OnePerOccurrence;
    task.subtasks = vec![
        Subtask::new("learn to cook"),
        Subtask::new("learn to sail"),
        Subtask::new("learn Rust macros"),
    ];
    task
}

#[test]
fn ticking_one_item_finishes_the_occurrence_and_moves_the_series() {
    let mut task = learning();
    let first = task.subtasks[0].id;

    let next = complete_item(&mut task, first, today());

    assert_eq!(next, Some(d(2026, 9, 26)), "the series should move on a week");
    assert!(task.subtasks[0].done);
    assert!(!task.is_done(), "the task itself is not finished");
    // The rest is what is left, not work that is outstanding.
    assert_eq!(task.remaining().count(), 2);
}

#[test]
fn an_ordinary_checklist_does_not_move_the_series() {
    let mut task = learning();
    task.checklist = Checklist::All;
    let first = task.subtasks[0].id;

    assert_eq!(complete_item(&mut task, first, today()), None);
    assert_eq!(task.due, Some(today()), "the date should not have moved");
    assert!(task.subtasks[0].done);
}

#[test]
fn un_ticking_an_item_is_a_correction_not_another_occurrence() {
    let mut task = learning();
    let first = task.subtasks[0].id;
    complete_item(&mut task, first, today());
    let moved_to = task.due;

    // Taking it back must not push the series forward a second time.
    assert_eq!(complete_item(&mut task, first, today()), None);
    assert!(!task.subtasks[0].done);
    assert_eq!(task.due, moved_to);
}

#[test]
fn a_backlog_survives_a_round_trip() {
    let mut store = Store::in_memory().unwrap();
    let task = learning();
    store.save(&task).unwrap();

    let loaded = store.get(task.id).unwrap().unwrap();
    assert_eq!(loaded.checklist, Checklist::OnePerOccurrence);
    assert_eq!(loaded.subtasks.len(), 3);
}

// ---------- the workout book ----------

#[test]
fn a_routine_holds_its_exercises_in_order() {
    let mut store = Store::in_memory().unwrap();
    let push = store.add_routine("Push").unwrap();
    store.add_routine("Pull").unwrap();

    for name in ["Incline dumbbell press", "Machine bench press", "Cable lateral raise"] {
        store.add_exercise(push.id, name).unwrap();
    }

    assert_eq!(store.routines().unwrap().len(), 2);
    let exercises = store.exercises(push.id).unwrap();
    assert_eq!(exercises.len(), 3);
    assert_eq!(exercises[0].name, "Incline dumbbell press");
    assert_eq!(exercises[2].name, "Cable lateral raise");
}

#[test]
fn a_session_records_sets_per_exercise() {
    let mut store = Store::in_memory().unwrap();
    let push = store.add_routine("Push").unwrap();
    let press = store.add_exercise(push.id, "Incline dumbbell press").unwrap();

    let log = store.start_session(push.id, today()).unwrap();
    for (index, (reps, weight)) in [(10, 22.5), (9, 22.5), (7, 25.0)].into_iter().enumerate() {
        store
            .save_set(
                log.session.id,
                SetEntry { exercise: press.id, index: index as u32 + 1, reps, weight },
            )
            .unwrap();
    }

    let sessions = store.sessions(push.id, 10).unwrap();
    assert_eq!(sessions.len(), 1);
    let sets = sessions[0].for_exercise(press.id);
    assert_eq!(sets.len(), 3);
    assert_eq!(sets[0].reps, 10);
    assert_eq!(sessions[0].top_weight(press.id), Some(25.0));
}

#[test]
fn starting_a_session_carries_the_last_one_across() {
    // Almost every set repeats; re-typing them is the thing a spreadsheet made
    // people do and the reason this exists.
    let mut store = Store::in_memory().unwrap();
    let push = store.add_routine("Push").unwrap();
    let press = store.add_exercise(push.id, "Bench").unwrap();

    let first = store.start_session(push.id, d(2026, 9, 12)).unwrap();
    store
        .save_set(
            first.session.id,
            SetEntry { exercise: press.id, index: 1, reps: 8, weight: 60.0 },
        )
        .unwrap();

    let second = store.start_session(push.id, today()).unwrap();
    assert_eq!(second.sets.len(), 1);
    assert_eq!(second.sets[0].reps, 8);
    assert_eq!(second.sets[0].weight, 60.0);

    // Editing the new one must not touch the old one.
    store
        .save_set(
            second.session.id,
            SetEntry { exercise: press.id, index: 1, reps: 8, weight: 62.5 },
        )
        .unwrap();
    let sessions = store.sessions(push.id, 10).unwrap();
    assert_eq!(sessions[0].for_exercise(press.id)[0].weight, 62.5);
    assert_eq!(sessions[1].for_exercise(press.id)[0].weight, 60.0);
}

#[test]
fn sessions_come_back_newest_first() {
    let mut store = Store::in_memory().unwrap();
    let push = store.add_routine("Push").unwrap();
    store.start_session(push.id, d(2026, 9, 5)).unwrap();
    store.start_session(push.id, d(2026, 9, 12)).unwrap();
    store.start_session(push.id, today()).unwrap();

    let dates: Vec<NaiveDate> =
        store.sessions(push.id, 10).unwrap().iter().map(|s| s.session.date).collect();
    assert_eq!(dates, [today(), d(2026, 9, 12), d(2026, 9, 5)]);
}

#[test]
fn deleting_a_routine_takes_its_history_with_it() {
    let mut store = Store::in_memory().unwrap();
    let push = store.add_routine("Push").unwrap();
    let press = store.add_exercise(push.id, "Bench").unwrap();
    let log = store.start_session(push.id, today()).unwrap();
    store
        .save_set(log.session.id, SetEntry { exercise: press.id, index: 1, reps: 5, weight: 80.0 })
        .unwrap();

    store.delete_routine(push.id).unwrap();

    assert!(store.routines().unwrap().is_empty());
    assert!(store.exercises(push.id).unwrap().is_empty());
    assert!(store.sessions(push.id, 10).unwrap().is_empty());
    assert!(store.sets(log.session.id).unwrap().is_empty());
}

#[test]
fn weights_read_and_write_the_way_they_are_spoken() {
    assert_eq!(workout::parse_weight("60"), Some(60.0));
    assert_eq!(workout::parse_weight("62.5"), Some(62.5));
    assert_eq!(workout::parse_weight("62,5"), Some(62.5));
    assert_eq!(workout::parse_weight("60kg"), Some(60.0));
    assert_eq!(workout::parse_weight("135 lbs"), Some(135.0));
    assert_eq!(workout::parse_weight(""), None);
    assert_eq!(workout::parse_weight("heavy"), None);

    assert_eq!(workout::format_weight(60.0), "60");
    assert_eq!(workout::format_weight(62.5), "62.5");
}

#[test]
fn volume_is_reps_times_weight() {
    let set = SetEntry { exercise: uuid::Uuid::new_v4(), index: 1, reps: 10, weight: 22.5 };
    assert_eq!(set.volume(), 225.0);
}
