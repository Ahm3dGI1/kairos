//! The vault as the app actually uses it: seed, mirror, notice an outside
//! edit, and rebuild a database that has been thrown away.
//!
//! These run the same sequence the Windows shell does at startup and after
//! every mutation, so a break here is a break in the app even though no window
//! is involved.

use chrono::NaiveDate;
use kairos_core::vault::Vault;
use kairos_core::{Priority, Recurrence, Store, Task};

fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 9, 20).unwrap()
}

/// A folder that cleans up after itself.
struct Temp(std::path::PathBuf);

impl Temp {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("kairos-lifecycle-{name}"));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn vault(&self) -> Vault {
        Vault::new(self.0.clone())
    }
    fn read(&self, relative: &str) -> String {
        std::fs::read_to_string(self.0.join(relative)).unwrap_or_default()
    }
    fn write(&self, relative: &str, text: &str) {
        let path = self.0.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// What `state::init` does when there is a database but no vault yet: the
/// existing data seeds the files rather than being replaced by them.
#[test]
fn a_first_run_seeds_the_vault_from_the_database() {
    let temp = Temp::new("seed");
    let vault = temp.vault();
    let mut store = Store::in_memory().unwrap();

    let mut task = Task::new("Buy milk", today());
    task.due = Some(today());
    store.save(&task).unwrap();

    assert!(!vault.exists(), "nothing there yet");
    vault.write(&store.snapshot().unwrap()).unwrap();

    assert!(vault.exists());
    assert!(temp.read("tasks/inbox.md").contains("Buy milk"));
    assert!(temp.read("README.md").contains("These files are the real data"));
}

/// The mirror that runs after every mutation.
#[test]
fn every_change_reaches_the_files() {
    let temp = Temp::new("mirror");
    let vault = temp.vault();
    let mut store = Store::in_memory().unwrap();

    let mut task = Task::new("Ship the release", today());
    task.due = Some(today());
    task.priority = Priority::High;
    store.save(&task).unwrap();
    vault.write(&store.snapshot().unwrap()).unwrap();
    assert!(temp.read("tasks/inbox.md").contains("- [ ] Ship the release"));

    store.complete(task.id, today()).unwrap();
    vault.write(&store.snapshot().unwrap()).unwrap();
    let text = temp.read("tasks/inbox.md");
    assert!(text.contains("- [x] Ship the release"), "completion shows as a ticked box:\n{text}");

    store.delete(task.id).unwrap();
    vault.write(&store.snapshot().unwrap()).unwrap();
    assert!(!temp.read("tasks/inbox.md").contains("Ship the release"));
}

/// The headline claim: type a line into a file in any editor and the app has
/// the task.
#[test]
fn a_line_typed_into_a_file_becomes_a_task() {
    let temp = Temp::new("external-add");
    let vault = temp.vault();
    let mut store = Store::in_memory().unwrap();
    vault.write(&store.snapshot().unwrap()).unwrap();

    let existing = temp.read("tasks/inbox.md");
    temp.write("tasks/inbox.md", &format!("{existing}- [ ] gym every day 5pm #health !p1\n"));

    // What the watcher does when it notices.
    store.restore(&vault.read(today()).unwrap()).unwrap();

    let tasks = store.all().unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].title, "gym");
    assert_eq!(tasks[0].recurrence, Some(Recurrence::Daily));
    assert_eq!(tasks[0].time, chrono::NaiveTime::from_hms_opt(17, 0, 0));
    assert_eq!(tasks[0].tags, ["health"]);
    assert_eq!(tasks[0].priority, Priority::High);
}

/// Changing a date by editing the text, which is the other half of the claim.
#[test]
fn editing_a_date_in_a_file_moves_the_task() {
    let temp = Temp::new("external-edit");
    let vault = temp.vault();
    let mut store = Store::in_memory().unwrap();

    let mut task = Task::new("Dentist", today());
    task.due = Some(today());
    store.save(&task).unwrap();
    vault.write(&store.snapshot().unwrap()).unwrap();

    let moved = temp.read("tasks/inbox.md").replace("due:2026-09-20", "due:2026-10-15");
    temp.write("tasks/inbox.md", &moved);
    store.restore(&vault.read(today()).unwrap()).unwrap();

    let stored = store.get(task.id).unwrap().expect("the id in the file kept its identity");
    assert_eq!(stored.due, NaiveDate::from_ymd_opt(2026, 10, 15));
    assert_eq!(stored.title, "Dentist");
}

/// Deleting the index must cost nothing.
#[test]
fn a_deleted_database_rebuilds_from_the_files() {
    let temp = Temp::new("rebuild");
    let vault = temp.vault();

    let mut original = Store::in_memory().unwrap();
    let mut task = Task::new("Write the spec", today());
    task.due = Some(today());
    task.notes = "Keep it short.".into();
    task.project = Some("Work".into());
    original.save(&task).unwrap();

    let habit = original.add_habit("Read", today()).unwrap();
    original.toggle_habit(habit.id, today()).unwrap();

    let routine = original.add_routine("Push").unwrap();
    let bench = original.add_exercise(routine.id, "Bench press").unwrap();
    let session = original.start_session(routine.id, today()).unwrap();
    original
        .save_set(
            session.session.id,
            kairos_core::workout::SetEntry { exercise: bench.id, index: 1, reps: 10, weight: 60.0 },
        )
        .unwrap();

    vault.write(&original.snapshot().unwrap()).unwrap();

    // The database is thrown away entirely.
    drop(original);
    let mut rebuilt = Store::in_memory().unwrap();
    rebuilt.restore(&vault.read(today()).unwrap()).unwrap();

    let tasks = rebuilt.all().unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].title, "Write the spec");
    assert_eq!(tasks[0].notes, "Keep it short.");
    assert_eq!(tasks[0].project.as_deref(), Some("Work"));

    let habits = rebuilt.habits(true).unwrap();
    assert_eq!(habits.len(), 1);
    assert!(rebuilt.day_log(today()).unwrap().is_done(habits[0].id));

    let routines = rebuilt.routines().unwrap();
    assert_eq!(routines.len(), 1);
    assert_eq!(routines[0].name, "Push");
    let sessions = rebuilt.sessions(routines[0].id, 10).unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].sets.len(), 1);
    assert_eq!(sessions[0].sets[0].reps, 10);
}

/// The watcher sees the app's own writes too. If a round trip were not a
/// no-op, every save would start an import/export ping-pong.
#[test]
fn the_app_does_not_chase_its_own_writes() {
    let temp = Temp::new("no-ping-pong");
    let vault = temp.vault();
    let mut store = Store::in_memory().unwrap();

    let mut task = Task::new("Gym", today());
    task.due = Some(today());
    task.recurrence = Some(Recurrence::Daily);
    store.save(&task).unwrap();

    vault.write(&store.snapshot().unwrap()).unwrap();
    for _ in 0..3 {
        store.restore(&vault.read(today()).unwrap()).unwrap();
        assert!(
            !vault.write(&store.snapshot().unwrap()).unwrap(),
            "an import followed by an export must not touch a file"
        );
    }
}

/// A habit invented in a month file, which is the most agent-friendly path in
/// the whole format.
#[test]
fn a_habit_written_into_a_month_file_starts_being_tracked() {
    let temp = Temp::new("new-habit");
    let vault = temp.vault();
    let mut store = Store::in_memory().unwrap();
    vault.write(&store.snapshot().unwrap()).unwrap();

    temp.write("habits/2026-09.md", "# September 2026\n\n## Ticks\n\n- Swim: 3 5 8\n");
    store.restore(&vault.read(today()).unwrap()).unwrap();

    let habits = store.habits(true).unwrap();
    assert_eq!(habits.len(), 1);
    assert_eq!(habits[0].name, "Swim");

    let on_the_fifth = NaiveDate::from_ymd_opt(2026, 9, 5).unwrap();
    assert!(store.day_log(on_the_fifth).unwrap().is_done(habits[0].id));
    assert!(!store.day_log(today()).unwrap().is_done(habits[0].id));

    // And it settles, like everything else.
    vault.write(&store.snapshot().unwrap()).unwrap();
    store.restore(&vault.read(today()).unwrap()).unwrap();
    assert_eq!(store.habits(true).unwrap().len(), 1, "it is not created twice");
}

/// A workout written the way someone would write it between sets.
#[test]
fn a_session_written_by_hand_is_read_back() {
    let temp = Temp::new("hand-workout");
    let vault = temp.vault();
    let mut store = Store::in_memory().unwrap();
    vault.write(&store.snapshot().unwrap()).unwrap();

    temp.write(
        "workouts/pull.md",
        "# Pull\n\n## Exercises\n\n- Deadlift\n- Row\n\n## 2026-09-19\n\n\
         - Deadlift: 5x100, 5x110, 3x120\n- Row: 10x50\n> Back felt fine.\n",
    );
    store.restore(&vault.read(today()).unwrap()).unwrap();

    let routines = store.routines().unwrap();
    assert_eq!(routines.len(), 1);
    assert_eq!(routines[0].name, "Pull");

    let exercises = store.exercises(routines[0].id).unwrap();
    assert_eq!(exercises.len(), 2);

    let sessions = store.sessions(routines[0].id, 10).unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].session.note, "Back felt fine.");
    let deadlift = sessions[0].for_exercise(exercises[0].id);
    assert_eq!(deadlift.len(), 3);
    assert_eq!((deadlift[2].reps, deadlift[2].weight), (3, 120.0));
}

/// Undo is app state, not vault data, so an import must not become a step you
/// can undo — and must not wipe the history of what you did in the app.
#[test]
fn an_outside_edit_is_not_an_undo_step() {
    let temp = Temp::new("undo");
    let vault = temp.vault();
    let mut store = Store::in_memory().unwrap();

    let task = Task::new("Something", today());
    store.save(&task).unwrap();
    vault.write(&store.snapshot().unwrap()).unwrap();
    assert!(store.can_undo().unwrap(), "adding a task is undoable");

    let existing = temp.read("tasks/inbox.md");
    temp.write("tasks/inbox.md", &format!("{existing}- [ ] typed in an editor\n"));
    store.restore(&vault.read(today()).unwrap()).unwrap();

    assert_eq!(store.all().unwrap().len(), 2, "the outside edit landed");
    assert!(store.can_undo().unwrap(), "and the app's own history is still there");
}

/// A habit and a task for the same thing: completing the task records the
/// habit, so "gym five days a week" is one act rather than two.
#[test]
fn completing_a_linked_task_ticks_its_habit() {
    let mut store = Store::in_memory().unwrap();
    let gym = store.add_habit("Gym", today()).unwrap();

    let mut task = Task::new("Gym", today());
    task.due = Some(today());
    task.recurrence = Some(Recurrence::WEEKDAYS);
    task.habit = Some(gym.id);
    store.save(&task).unwrap();

    assert!(!store.day_log(today()).unwrap().is_done(gym.id));
    store.complete(task.id, today()).unwrap();
    assert!(store.day_log(today()).unwrap().is_done(gym.id), "the habit was recorded");

    // Completing again on the same day is one day done, not a day undone.
    store.complete(task.id, today()).unwrap();
    assert!(store.day_log(today()).unwrap().is_done(gym.id));
}

/// An unlinked task leaves the habits alone.
#[test]
fn completing_an_ordinary_task_records_no_habit() {
    let mut store = Store::in_memory().unwrap();
    let gym = store.add_habit("Gym", today()).unwrap();

    let mut task = Task::new("Something else", today());
    task.due = Some(today());
    store.save(&task).unwrap();
    store.complete(task.id, today()).unwrap();

    assert!(!store.day_log(today()).unwrap().is_done(gym.id));
}

/// The link survives the vault, which is what makes it real data rather than
/// a detail of the database.
#[test]
fn a_link_survives_a_trip_through_the_files() {
    let temp = Temp::new("link");
    let vault = temp.vault();
    let mut store = Store::in_memory().unwrap();

    let gym = store.add_habit("Gym", today()).unwrap();
    let mut task = Task::new("Gym", today());
    task.due = Some(today());
    task.habit = Some(gym.id);
    store.save(&task).unwrap();

    vault.write(&store.snapshot().unwrap()).unwrap();
    assert!(temp.read("tasks/inbox.md").contains("habit:Gym"));

    store.restore(&vault.read(today()).unwrap()).unwrap();
    let back = store.get(task.id).unwrap().unwrap();
    assert_eq!(back.habit, Some(gym.id));
}
