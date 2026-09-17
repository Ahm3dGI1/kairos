//! The local store: persistence, the saved views, and undo.

use chrono::NaiveDate;
use mtodo_core::store::UndoKind;
use mtodo_core::{
    parse_at, Exception, Filter, Priority, Recurrence, Sort, Store, Subtask, Task, WeekdaySet,
};

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

fn today() -> NaiveDate {
    d(2026, 9, 13)
}

fn store() -> Store {
    Store::in_memory().expect("open in-memory store")
}

/// A task with every field populated, to prove the whole model round-trips.
fn full_task() -> Task {
    let mut task = Task::new("submit report", today());
    task.notes = "include the Q3 numbers".into();
    task.due = Some(d(2026, 9, 14));
    task.time =
        NaiveDate::from_ymd_opt(2026, 9, 14).unwrap().and_hms_opt(9, 30, 0).map(|t| t.time());
    task.recurrence = Some(Recurrence::Weekly { days: WeekdaySet::from_day(chrono::Weekday::Mon) });
    task.exceptions.push(Exception::skip(d(2026, 9, 21)));
    task.priority = Priority::High;
    task.add_tag("work");
    task.add_tag("urgent");
    task.project = Some("q3".into());
    task.subtasks.push(Subtask::new("pull the numbers"));
    task
}

#[test]
fn a_task_survives_a_round_trip_with_every_field_intact() {
    let mut store = store();
    let task = full_task();
    store.save(&task).unwrap();

    let loaded = store.get(task.id).unwrap().expect("task should be there");
    assert_eq!(loaded, task);
}

#[test]
fn saving_twice_updates_rather_than_duplicating() {
    let mut store = store();
    let mut task = Task::new("draft", today());
    store.save(&task).unwrap();

    task.title = "draft v2".into();
    store.save(&task).unwrap();

    assert_eq!(store.all().unwrap().len(), 1);
    assert_eq!(store.get(task.id).unwrap().unwrap().title, "draft v2");
}

#[test]
fn a_parsed_line_can_be_stored_as_it_came_out() {
    let mut store = store();
    let now = today().and_hms_opt(9, 0, 0).unwrap();
    let task = parse_at("submit report every monday 9am @work #urgent !p1", now).task;
    store.save(&task).unwrap();

    let loaded = store.get(task.id).unwrap().unwrap();
    assert_eq!(loaded.title, "submit report");
    assert_eq!(loaded.priority, Priority::High);
    assert_eq!(loaded.project.as_deref(), Some("work"));
    assert_eq!(loaded.tags, ["urgent"]);
}

#[test]
fn the_database_persists_across_reopening() {
    let dir = std::env::temp_dir().join(format!("mtodo-test-{}", uuid_like()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("tasks.db");

    let task = full_task();
    {
        let mut store = Store::open(&path).unwrap();
        store.save(&task).unwrap();
    }
    {
        let store = Store::open(&path).unwrap();
        assert_eq!(store.get(task.id).unwrap().as_ref(), Some(&task));
    }
    std::fs::remove_dir_all(&dir).ok();
}

/// A unique-enough suffix for a temp directory without pulling in a dependency
/// the library does not otherwise need here.
fn uuid_like() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

#[test]
fn views_select_the_right_tasks() {
    let mut store = store();

    let mut overdue = Task::new("overdue thing", today());
    overdue.due = Some(d(2026, 9, 10));
    store.save(&overdue).unwrap();

    let mut today_task = Task::new("today thing", today());
    today_task.due = Some(today());
    store.save(&today_task).unwrap();

    let mut soon = Task::new("soon thing", today());
    soon.due = Some(d(2026, 9, 17));
    store.save(&soon).unwrap();

    let mut far = Task::new("far thing", today());
    far.due = Some(d(2026, 12, 1));
    store.save(&far).unwrap();

    let mut someday = Task::new("someday thing", today());
    someday.project = Some("ideas".into());
    someday.add_tag("maybe");
    store.save(&someday).unwrap();

    let titles = |filter: Filter| -> Vec<String> {
        store.query(&filter, Sort::Due, today()).unwrap().into_iter().map(|t| t.title).collect()
    };

    assert_eq!(titles(Filter::Overdue), ["overdue thing"]);
    // Today includes overdue work: it is what most needs attention.
    assert_eq!(titles(Filter::Today), ["overdue thing", "today thing"]);
    assert_eq!(titles(Filter::Next { days: 7 }), ["overdue thing", "today thing", "soon thing"]);
    assert_eq!(titles(Filter::Project("ideas".into())), ["someday thing"]);
    assert_eq!(titles(Filter::Tag("maybe".into())), ["someday thing"]);
    assert_eq!(titles(Filter::Search("far".into())), ["far thing"]);
    assert_eq!(titles(Filter::All).len(), 5);
    assert!(titles(Filter::Completed).is_empty());
}

#[test]
fn a_recurring_task_shows_up_on_every_occurrence_day() {
    let mut store = store();
    let mut task = Task::new("standup", today());
    task.due = Some(d(2026, 9, 14));
    task.recurrence = Some(Recurrence::WEEKDAYS);
    store.save(&task).unwrap();

    // Monday the 14th is an occurrence; Sunday the 13th is not.
    let monday = store.query(&Filter::Today, Sort::Due, d(2026, 9, 14)).unwrap();
    assert_eq!(monday.len(), 1);

    let sunday = store.query(&Filter::Today, Sort::Due, d(2026, 9, 13)).unwrap();
    assert!(sunday.is_empty(), "no weekday occurrence falls on a Sunday");
}

#[test]
fn completing_a_recurring_task_keeps_it_in_the_list() {
    let mut store = store();
    let mut task = Task::new("gym", today());
    task.due = Some(today());
    task.recurrence = Some(Recurrence::Daily);
    store.save(&task).unwrap();

    let after = store.complete(task.id, today()).unwrap();
    assert!(!after.is_done(), "a series should not close on one completion");
    assert_eq!(after.due, Some(d(2026, 9, 14)));

    // Still open, so it is still in All.
    assert_eq!(store.query(&Filter::All, Sort::Due, today()).unwrap().len(), 1);
}

#[test]
fn completing_a_one_off_task_moves_it_to_completed() {
    let mut store = store();
    let mut task = Task::new("buy milk", today());
    task.due = Some(today());
    store.save(&task).unwrap();

    store.complete(task.id, today()).unwrap();

    assert!(store.query(&Filter::All, Sort::Due, today()).unwrap().is_empty());
    assert_eq!(store.query(&Filter::Completed, Sort::Due, today()).unwrap().len(), 1);
}

#[test]
fn undo_brings_back_a_deleted_task() {
    let mut store = store();
    let task = full_task();
    store.save(&task).unwrap();
    store.delete(task.id).unwrap();
    assert!(store.get(task.id).unwrap().is_none());

    let undone = store.undo().unwrap().expect("something to undo");
    assert_eq!(undone.kind, UndoKind::Delete);
    assert_eq!(store.get(task.id).unwrap().as_ref(), Some(&task));
}

#[test]
fn undo_reverses_a_completion() {
    let mut store = store();
    let mut task = Task::new("buy milk", today());
    task.due = Some(today());
    store.save(&task).unwrap();

    store.complete(task.id, today()).unwrap();
    assert!(store.get(task.id).unwrap().unwrap().is_done());

    let undone = store.undo().unwrap().expect("something to undo");
    assert_eq!(undone.kind, UndoKind::Complete);
    assert!(!store.get(task.id).unwrap().unwrap().is_done());
}

#[test]
fn undo_of_a_creation_removes_the_task() {
    let mut store = store();
    let task = Task::new("oops", today());
    store.save(&task).unwrap();

    let undone = store.undo().unwrap().expect("something to undo");
    assert_eq!(undone.kind, UndoKind::Create);
    assert!(undone.task.is_none());
    assert!(store.get(task.id).unwrap().is_none());
}

#[test]
fn undo_unwinds_one_step_at_a_time() {
    let mut store = store();
    let mut task = Task::new("first", today());
    store.save(&task).unwrap();

    task.title = "second".into();
    store.save(&task).unwrap();

    task.title = "third".into();
    store.save(&task).unwrap();

    store.undo().unwrap();
    assert_eq!(store.get(task.id).unwrap().unwrap().title, "second");

    store.undo().unwrap();
    assert_eq!(store.get(task.id).unwrap().unwrap().title, "first");

    store.undo().unwrap();
    assert!(store.get(task.id).unwrap().is_none());

    // Nothing left.
    assert!(store.undo().unwrap().is_none());
    assert!(!store.can_undo().unwrap());
}

#[test]
fn projects_and_tags_list_what_is_in_use() {
    let mut store = store();

    let mut a = Task::new("a", today());
    a.project = Some("work".into());
    a.add_tag("urgent");
    store.save(&a).unwrap();

    let mut b = Task::new("b", today());
    b.project = Some("home".into());
    b.add_tag("errands");
    b.add_tag("urgent");
    store.save(&b).unwrap();

    assert_eq!(store.projects().unwrap(), ["home", "work"]);
    assert_eq!(store.tags().unwrap(), ["errands", "urgent"]);
}

#[test]
fn sorting_puts_undated_tasks_last() {
    let mut store = store();

    let mut undated = Task::new("undated", today());
    undated.priority = Priority::High;
    store.save(&undated).unwrap();

    let mut dated = Task::new("dated", today());
    dated.due = Some(d(2026, 9, 20));
    store.save(&dated).unwrap();

    let titles: Vec<String> = store
        .query(&Filter::All, Sort::Due, today())
        .unwrap()
        .into_iter()
        .map(|t| t.title)
        .collect();
    assert_eq!(titles, ["dated", "undated"]);
}
