use chrono::NaiveDate;
use kairos_core::daily::{HabitKind, JournalEntry, MonthJournal};
use kairos_core::vault::Vault;
use kairos_core::{complete_occurrence, Exception, Recurrence, Snapshot, Store, Task, WeekdaySet};

fn date(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}
struct Temp(std::path::PathBuf);
impl Temp {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!("kairos-regression-{}", uuid::Uuid::new_v4())))
    }
    fn vault(&self) -> Vault {
        Vault::new(&self.0)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn colliding_project_names_and_inbox_survive() {
    let temp = Temp::new();
    let snapshot = Snapshot {
        tasks: [None, Some("Inbox"), Some("Work/Home"), Some("Work-Home"), Some("CON")]
            .into_iter()
            .map(|name| {
                let mut t = Task::new(name.unwrap_or("unfiled"), date("2026-09-23"));
                t.project = name.map(str::to_string);
                t
            })
            .collect(),
        ..Default::default()
    };
    temp.vault().write(&snapshot).unwrap();
    let read = temp.vault().read(date("2026-09-23")).unwrap();
    assert_eq!(read.tasks.len(), snapshot.tasks.len());
    for task in snapshot.tasks {
        assert!(read.tasks.contains(&task), "lost {}", task.title);
    }
}

#[test]
fn unreadable_utf8_aborts_import_and_export() {
    let temp = Temp::new();
    let vault = temp.vault();
    vault.write(&Snapshot::default()).unwrap();
    std::fs::write(temp.0.join("tasks/broken.md"), [255, 254, 128]).unwrap();
    assert!(vault.read(date("2026-09-23")).is_err());
    assert!(vault.write(&Snapshot::default()).is_err());
    assert_eq!(std::fs::read(temp.0.join("tasks/broken.md")).unwrap(), [255, 254, 128]);
}

#[test]
fn numeric_units_and_kind_changes_keep_history() {
    let temp = Temp::new();
    let vault = temp.vault();
    let mut store = Store::in_memory().unwrap();
    let day = date("2026-09-23");
    let mut habit = store.add_habit("Distance", day).unwrap();
    habit.kind = HabitKind::Number { unit: "km".into() };
    store.save_habit(&habit).unwrap();
    let mut log = store.day_log(day).unwrap();
    log.set_value(habit.id, Some(5.125));
    store.save_day_log(&log).unwrap();
    vault.write(&store.snapshot().unwrap()).unwrap();
    let read = vault.read(day).unwrap();
    assert_eq!(read.day_logs[0].value(habit.id), Some(5.125));
    habit.kind = HabitKind::Check;
    store.save_habit(&habit).unwrap();
    vault.write(&store.snapshot().unwrap()).unwrap();
    let read = vault.read(day).unwrap();
    assert_eq!(read.day_logs[0].value(habit.id), Some(5.125));
}

#[test]
fn journal_ids_and_markdown_headings_round_trip() {
    let temp = Temp::new();
    let mut journal = MonthJournal::new(2026, 9);
    let mut entry = JournalEntry::new("Today");
    entry.body = "Intro\n## Heading\n### Subheading\n> quote\n\nFinal".into();
    journal.entries.push(entry);
    let snapshot = Snapshot { journals: vec![journal.clone()], ..Default::default() };
    temp.vault().write(&snapshot).unwrap();
    let read = temp.vault().read(date("2026-09-23")).unwrap();
    assert_eq!(read.journals, [journal]);
}

#[test]
fn completion_keeps_monthly_and_weekly_anchors() {
    let mut monthly = Task::new("Rent", date("2027-01-31"));
    monthly.due = Some(date("2027-01-31"));
    monthly.recurrence = Some(Recurrence::Monthly { day: None });
    assert_eq!(complete_occurrence(&mut monthly, date("2027-01-31")), Some(date("2027-02-28")));
    assert_eq!(complete_occurrence(&mut monthly, date("2027-02-28")), Some(date("2027-03-31")));
    let mut weekly = Task::new("Weekly", date("2026-09-21"));
    weekly.due = Some(date("2026-09-21"));
    weekly.recurrence = Some(Recurrence::Weekly { days: WeekdaySet::EMPTY });
    weekly.exceptions.push(Exception::move_to(date("2026-09-28"), date("2026-09-29")));
    assert_eq!(complete_occurrence(&mut weekly, date("2026-09-21")), Some(date("2026-09-29")));
    assert_eq!(complete_occurrence(&mut weekly, date("2026-09-29")), Some(date("2026-10-05")));
}

#[test]
fn linked_completion_undo_redo_and_backlog_are_consistent() {
    let mut store = Store::in_memory().unwrap();
    let day = date("2026-09-23");
    let habit = store.add_habit("Learn", day).unwrap();
    let mut task = Task::new("Learn", day);
    task.due = Some(day);
    task.habit = Some(habit.id);
    task.recurrence = Some(Recurrence::Daily);
    task.checklist = kairos_core::Checklist::OnePerOccurrence;
    let sub = kairos_core::Subtask::new("Cook");
    let id = sub.id;
    task.subtasks.push(sub);
    store.save(&task).unwrap();
    store.complete_subtask(task.id, id, day).unwrap();
    assert!(store.day_log(day).unwrap().is_done(habit.id));
    store.undo().unwrap();
    assert!(!store.day_log(day).unwrap().is_done(habit.id));
    assert!(!store.get(task.id).unwrap().unwrap().subtasks[0].done);
    store.redo().unwrap();
    assert!(store.day_log(day).unwrap().is_done(habit.id));
    assert!(store.get(task.id).unwrap().unwrap().subtasks[0].done);
}

#[test]
fn giant_date_inputs_do_not_panic() {
    for text in ["plan in 4294967295 days", "plan in 4294967295 weeks", "plan in 4294967295 years"]
    {
        let result = kairos_core::parse_at(text, date("2026-09-23").and_hms_opt(9, 0, 0).unwrap());
        assert!(result.task.due.is_none());
    }
}

#[test]
fn widget_layout_preserves_views_filters_and_geometry() {
    use kairos_core::widgets::{WidgetLayout, WidgetSpec, WidgetView};
    let temp = Temp::new();
    let layout = WidgetLayout {
        restore_on_start: true,
        widgets: vec![WidgetSpec {
            id: uuid::Uuid::new_v4(),
            view: WidgetView::Agenda,
            project: Some("Work".into()),
            routine: None,
            x: -1200,
            y: 20,
            width: 600.0,
            height: 450.0,
            pinned: true,
        }],
    };
    let path = temp.0.join("widget-layout.json");
    layout.save(&path).unwrap();
    assert_eq!(WidgetLayout::load(&path).unwrap(), layout);
    std::fs::write(&path, "bad json").unwrap();
    assert!(WidgetLayout::load(&path).is_err());
}

#[test]
fn workout_ids_survive_rebuild() {
    let temp = Temp::new();
    let mut store = Store::in_memory().unwrap();
    let routine = store.add_routine("Push").unwrap();
    let exercise = store.add_exercise(routine.id, "Bench").unwrap();
    let session = store.start_session(routine.id, date("2026-09-23")).unwrap();
    store
        .save_set(
            session.session.id,
            kairos_core::workout::SetEntry {
                exercise: exercise.id,
                index: 1,
                reps: 8,
                weight: 60.0,
            },
        )
        .unwrap();
    let before = store.snapshot().unwrap();
    temp.vault().write(&before).unwrap();
    assert_eq!(temp.vault().read(date("2026-09-23")).unwrap(), before);
}

#[test]
fn a_failed_export_rolls_back_completed_file_writes() {
    let temp = Temp::new();
    let vault = temp.vault();
    let day = date("2026-09-23");
    let before = Snapshot { tasks: vec![Task::new("original", day)], ..Default::default() };
    vault.write(&before).unwrap();
    let fingerprint = vault.fingerprint().unwrap();
    let mut after = before.clone();
    after.tasks[0].title = "changed".into();
    let mut extra = Task::new("blocked", day);
    extra.project = Some("zzz".into());
    after.tasks.push(extra);
    std::fs::create_dir(temp.0.join("tasks/zzz.md")).unwrap();
    assert!(vault.write(&after).is_err());
    assert_eq!(vault.fingerprint().unwrap(), fingerprint);
}

#[test]
fn external_edits_invalidate_only_affected_task_history() {
    let mut store = Store::in_memory().unwrap();
    let day = date("2026-09-23");
    let task = Task::new("original", day);
    store.save(&task).unwrap();
    let mut snapshot = store.snapshot().unwrap();
    snapshot.tasks[0].title = "edited externally".into();
    store.restore_external(&snapshot).unwrap();
    assert!(!store.can_undo().unwrap());
    assert_eq!(store.get(task.id).unwrap().unwrap().title, "edited externally");
}
