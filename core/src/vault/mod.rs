//! The vault: everything the app knows, as plain text files on disk.
//!
//! This is the system of record. The SQLite database is an index built from
//! these files — delete it and the next launch rebuilds it; delete these and
//! the data is gone. That inversion is the whole point: a todo list you cannot
//! open in a text editor is a todo list you do not own, and the project's
//! premise is that you do.
//!
//! ```text
//! <vault>/
//!   README.md            what the formats are — written for whoever opens the folder
//!   settings.json        the switches (see [`crate::settings`])
//!   tasks/inbox.md       one file per project, one Markdown checklist item per task
//!   tasks/health.md
//!   habits/habits.md     the habits themselves
//!   habits/2026-09.md    a month of ticks, numbers and journal
//!   workouts/push.md     a routine, its exercises, and every session
//! ```
//!
//! Writing is **content-addressed**: a file is only touched when what it should
//! contain differs from what it does. That keeps the file watcher from chasing
//! its own tail, and keeps a vault under version control from churning.

pub mod habits;
pub mod tasks;
pub mod workouts;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use chrono::{Datelike, NaiveDate};

use crate::daily::{Habit, MonthJournal};
use crate::store::Snapshot;

const TASKS_DIR: &str = "tasks";
const HABITS_DIR: &str = "habits";
const WORKOUTS_DIR: &str = "workouts";
const HABITS_FILE: &str = "habits.md";

/// A folder of Markdown files holding everything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vault {
    root: PathBuf,
}

impl Vault {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Whether the folder holds a vault already, as opposed to being absent or
    /// empty. A first run seeds an empty vault from the database; an existing
    /// one is read instead.
    pub fn exists(&self) -> bool {
        [TASKS_DIR, HABITS_DIR, WORKOUTS_DIR].iter().any(|dir| {
            std::fs::read_dir(self.root.join(dir))
                .map(|entries| entries.flatten().any(|e| is_markdown(&e.path())))
                .unwrap_or(false)
        })
    }

    /// Writes the snapshot out, touching only the files whose contents differ.
    /// Returns whether anything actually changed on disk.
    pub fn write(&self, snapshot: &Snapshot) -> std::io::Result<bool> {
        let mut changed = false;

        std::fs::create_dir_all(&self.root)?;
        changed |= write_if_changed(&self.root.join("README.md"), README)?;

        // ---- tasks, one file per project
        let mut by_project: HashMap<Option<String>, Vec<crate::task::Task>> = HashMap::new();
        by_project.entry(None).or_default();
        for task in &snapshot.tasks {
            by_project.entry(task.project.clone()).or_default().push(task.clone());
        }

        let dir = self.root.join(TASKS_DIR);
        std::fs::create_dir_all(&dir)?;
        let mut expected = HashSet::new();
        for (project, mut group) in by_project {
            // A stable order, so the file does not reshuffle between writes.
            group.sort_by_key(|t| (t.due, t.created_at, t.id));
            let name = format!("{}.md", tasks::slug(project.as_deref()));
            expected.insert(name.clone());
            changed |=
                write_if_changed(&dir.join(name), &tasks::write_file(project.as_deref(), &group))?;
        }
        changed |= prune(&dir, &expected)?;

        // ---- habits: the definitions, then a file per month
        let dir = self.root.join(HABITS_DIR);
        std::fs::create_dir_all(&dir)?;
        let mut habits = snapshot.habits.clone();
        habits.sort_by_key(|h| h.position);
        let mut expected = HashSet::from([HABITS_FILE.to_string()]);
        changed |= write_if_changed(&dir.join(HABITS_FILE), &habits::write_habits(&habits))?;

        let mut months: HashSet<(i32, u32)> = HashSet::new();
        for log in &snapshot.day_logs {
            if !log.is_empty() {
                months.insert((log.date.year(), log.date.month()));
            }
        }
        for journal in &snapshot.journals {
            if !journal.is_empty() {
                months.insert((journal.year, journal.month));
            }
        }
        for (year, month) in months {
            let logs: Vec<_> = snapshot
                .day_logs
                .iter()
                .filter(|log| log.date.year() == year && log.date.month() == month)
                .cloned()
                .collect();
            let journal = snapshot
                .journals
                .iter()
                .find(|j| j.year == year && j.month == month)
                .cloned()
                .unwrap_or_else(|| MonthJournal::new(year, month));

            let name = habits::month_filename(year, month);
            expected.insert(name.clone());
            changed |= write_if_changed(
                &dir.join(name),
                &habits::write_month(year, month, &habits, &logs, &journal),
            )?;
        }
        changed |= prune(&dir, &expected)?;

        // ---- workouts, one file per routine
        let dir = self.root.join(WORKOUTS_DIR);
        std::fs::create_dir_all(&dir)?;
        let mut expected = HashSet::new();
        for routine in &snapshot.routines {
            let mut exercises: Vec<_> =
                snapshot.exercises.iter().filter(|e| e.routine == routine.id).cloned().collect();
            exercises.sort_by_key(|e| e.position);
            let sessions: Vec<_> = snapshot
                .sessions
                .iter()
                .filter(|log| log.session.routine == routine.id)
                .cloned()
                .collect();

            let name = format!("{}.md", workouts::slug(&routine.name));
            expected.insert(name.clone());
            changed |= write_if_changed(
                &dir.join(name),
                &workouts::write_routine(routine, &exercises, &sessions),
            )?;
        }
        changed |= prune(&dir, &expected)?;

        Ok(changed)
    }

    /// Reads the whole vault back.
    ///
    /// `today` dates anything a hand-written line left unsaid. Unreadable files
    /// are skipped rather than failing the import: one malformed file should
    /// cost you that file, not the app.
    pub fn read(&self, today: NaiveDate) -> std::io::Result<Snapshot> {
        let mut snapshot = Snapshot::default();

        for (path, text) in markdown_in(&self.root.join(TASKS_DIR)) {
            let (heading, mut found) = tasks::read_file(&text, today);
            let project = heading.or_else(|| {
                let stem = path.file_stem()?.to_string_lossy().to_string();
                (stem != "inbox").then_some(stem)
            });
            for task in &mut found {
                task.project = project.clone();
            }
            snapshot.tasks.append(&mut found);
        }

        // Definitions first, so the month files can resolve names to ids.
        let dir = self.root.join(HABITS_DIR);
        if let Ok(text) = std::fs::read_to_string(dir.join(HABITS_FILE)) {
            snapshot.habits = habits::read_habits(&text, today);
        }
        let mut known: HashMap<String, uuid::Uuid> =
            snapshot.habits.iter().map(|h| (h.name.to_lowercase(), h.id)).collect();

        for (path, text) in markdown_in(&dir) {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            let Some((year, month)) = habits::month_of(&name) else { continue };
            let mut found = habits::read_month(&text, year, month, &known);

            // A month file naming a habit no definition knew about creates it —
            // which is what makes `- Swim: 5 6 7` in a text editor enough.
            for name in found.unknown_habits {
                let id = habits::derived_habit_id(&name);
                if known.values().any(|existing| *existing == id) {
                    continue;
                }
                known.insert(name.to_lowercase(), id);
                snapshot.habits.push(Habit {
                    id,
                    name,
                    created_at: NaiveDate::from_ymd_opt(year, month, 1).unwrap_or(today),
                    archived: false,
                    position: snapshot.habits.len() as i64,
                });
            }

            snapshot.day_logs.append(&mut found.logs);
            if !found.journal.is_empty() {
                snapshot.journals.push(MonthJournal { year, month, entries: found.journal });
            }
        }
        snapshot.day_logs.sort_by_key(|log| log.date);

        for (index, (path, text)) in markdown_in(&self.root.join(WORKOUTS_DIR)).enumerate() {
            let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
            let mut file = workouts::read_routine(&text, &stem);
            file.routine.position = index as i64;
            snapshot.routines.push(file.routine);
            snapshot.exercises.append(&mut file.exercises);
            snapshot.sessions.append(&mut file.sessions);
        }

        Ok(snapshot)
    }
}

/// Writes only if the file does not already say exactly this.
fn write_if_changed(path: &Path, contents: &str) -> std::io::Result<bool> {
    if std::fs::read_to_string(path).is_ok_and(|existing| existing == contents) {
        return Ok(false);
    }
    std::fs::write(path, contents)?;
    Ok(true)
}

/// Removes the Markdown files in an app-managed folder that the snapshot no
/// longer accounts for — a renamed project, a deleted routine. Only `.md`
/// files directly in the folder, so anything else a user keeps there survives.
fn prune(dir: &Path, expected: &HashSet<String>) -> std::io::Result<bool> {
    let mut changed = false;
    let Ok(entries) = std::fs::read_dir(dir) else { return Ok(false) };
    for entry in entries.flatten() {
        let path = entry.path();
        if !is_markdown(&path) {
            continue;
        }
        let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
        if expected.contains(&name) || name.eq_ignore_ascii_case("README.md") {
            continue;
        }
        std::fs::remove_file(&path)?;
        changed = true;
    }
    Ok(changed)
}

fn is_markdown(path: &Path) -> bool {
    path.is_file() && path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
}

/// Every `.md` file in a folder, sorted by name so reads are deterministic.
fn markdown_in(dir: &Path) -> impl Iterator<Item = (PathBuf, String)> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| is_markdown(path))
        .collect();
    paths.sort();
    paths.into_iter().filter_map(|path| {
        let text = std::fs::read_to_string(&path).ok()?;
        Some((path, text))
    })
}

const README: &str = r#"# Your Kairos vault

These files are the real data. The app's database is an index built from them:
delete it and the next launch rebuilds it from here. Edit anything below in any
editor — or point an AI agent at the folder — and the app picks the change up
within a couple of seconds.

## tasks/

One file per project; `inbox.md` is the one with no project. Each task is an
ordinary Markdown checklist item:

    - [ ] Gym due:2026-09-20 at:17:00 repeat:"every day" #fitness !p1 ^<id>
      - [ ] a subtask
      > a note

Fields, all optional: `due:` a date, `at:` a time, `repeat:` a phrase in
quotes, `skip:` a date this occurrence does not happen, `moved:from>to`,
`done:` the date it was finished (with `[x]`), `list:backlog` to make the
subtasks a backlog, `#tag`, `!p1`/`!p2`/`!p3`, `created:` and `^<id>`.

Two things worth knowing:

* **Metadata is a suffix.** Reading stops at the first thing from the end of
  the line it does not recognize, so a `#` or a `due:` mid-sentence stays in
  the title.
* **A line with no `^<id>` is treated as new**, and goes through the same
  natural-language parser as the app's capture bar. So you can type

      - [ ] gym every day 5pm #health

  into any file here and get exactly the task typing that into the app makes.
  Lines that already have an id are read literally, so editing a title never
  silently moves a date.

## habits/

`habits.md` lists the habits; anything under an `## Archived` heading is
retired. `2026-09.md` is one month: which days each habit was ticked, a table
of the two numbers, and the month's journal. Naming a habit in a month file is
enough to create it.

## workouts/

One file per routine. `## Exercises` lists the movements; every other `##` is a
date, and under it one line per exercise: `- Bench press: 10x60, 8x65, 6x70`.
A bare number is a bodyweight set. Nothing here needs an id.

## settings.json

Every switch on the app's Settings page. A missing key means the default, so
you can delete anything you have not changed.
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;
    use crate::task::Task;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, 20).unwrap()
    }

    struct TempVault(PathBuf);

    impl TempVault {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!("kairos-vault-{name}"));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn vault(&self) -> Vault {
            Vault::new(self.0.clone())
        }
    }

    impl Drop for TempVault {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn populated() -> Store {
        let mut store = Store::in_memory().unwrap();

        let mut gym = Task::new("Gym", today());
        gym.due = Some(today());
        gym.time = chrono::NaiveTime::from_hms_opt(17, 0, 0);
        gym.recurrence = Some(crate::task::Recurrence::Daily);
        gym.add_tag("fitness");
        gym.priority = crate::task::Priority::High;
        store.save(&gym).unwrap();

        let mut report = Task::new("Write the report", today());
        report.project = Some("Work".into());
        report.notes = "Two pages, no more.".into();
        report.subtasks = vec![crate::task::Subtask::new("outline")];
        store.save(&report).unwrap();

        let habit = store.add_habit("Read", today()).unwrap();
        store.toggle_habit(habit.id, today()).unwrap();

        let mut log = store.day_log(today()).unwrap();
        log.sleep_minutes = Some(450);
        store.save_day_log(&log).unwrap();

        let routine = store.add_routine("Push").unwrap();
        let bench = store.add_exercise(routine.id, "Bench press").unwrap();
        let session = store.start_session(routine.id, today()).unwrap();
        store
            .save_set(
                session.session.id,
                crate::workout::SetEntry { exercise: bench.id, index: 1, reps: 10, weight: 60.0 },
            )
            .unwrap();

        store
    }

    #[test]
    fn a_database_survives_a_trip_through_the_files() {
        let temp = TempVault::new("roundtrip");
        let vault = temp.vault();
        let original = populated().snapshot().unwrap();

        vault.write(&original).unwrap();
        let back = vault.read(today()).unwrap();

        assert_eq!(back.tasks.len(), original.tasks.len());
        let gym = back.tasks.iter().find(|t| t.title == "Gym").expect("gym survived");
        assert_eq!(gym.recurrence, Some(crate::task::Recurrence::Daily));
        assert_eq!(gym.priority, crate::task::Priority::High);
        assert_eq!(gym.tags, ["fitness"]);
        assert_eq!(gym.project, None);

        let report = back.tasks.iter().find(|t| t.title == "Write the report").unwrap();
        assert_eq!(report.project.as_deref(), Some("Work"));
        assert_eq!(report.notes, "Two pages, no more.");
        assert_eq!(report.subtasks.len(), 1);

        assert_eq!(back.habits.len(), 1);
        assert_eq!(back.habits[0].name, "Read");
        assert_eq!(back.day_logs.len(), 1);
        assert_eq!(back.day_logs[0].sleep_minutes, Some(450));
        assert!(back.day_logs[0].is_done(back.habits[0].id));

        assert_eq!(back.routines.len(), 1);
        assert_eq!(back.exercises.len(), 1);
        assert_eq!(back.sessions.len(), 1);
        assert_eq!(back.sessions[0].sets.len(), 1);
    }

    #[test]
    fn a_second_write_of_unchanged_data_touches_nothing() {
        let temp = TempVault::new("idempotent");
        let vault = temp.vault();
        let snapshot = populated().snapshot().unwrap();

        assert!(vault.write(&snapshot).unwrap(), "the first write creates the files");
        assert!(
            !vault.write(&snapshot).unwrap(),
            "writing the same data again must not touch a single file"
        );
    }

    /// The loop that would otherwise run forever: app writes, watcher fires,
    /// app imports, app writes again.
    #[test]
    fn importing_then_exporting_settles() {
        let temp = TempVault::new("settles");
        let vault = temp.vault();
        let snapshot = populated().snapshot().unwrap();
        vault.write(&snapshot).unwrap();

        let imported = vault.read(today()).unwrap();
        assert!(
            !vault.write(&imported).unwrap(),
            "a round trip through the files must produce the same files"
        );
    }

    #[test]
    fn a_hand_written_file_becomes_real_tasks() {
        let temp = TempVault::new("handwritten");
        let vault = temp.vault();
        std::fs::create_dir_all(temp.0.join(TASKS_DIR)).unwrap();
        std::fs::write(
            temp.0.join(TASKS_DIR).join("reading.md"),
            "# Reading\n\n- [ ] finish the Rust book every week\n- [x] start it\n",
        )
        .unwrap();

        let snapshot = vault.read(today()).unwrap();
        assert_eq!(snapshot.tasks.len(), 2);
        let unfinished = snapshot.tasks.iter().find(|t| !t.is_done()).unwrap();
        assert_eq!(unfinished.title, "finish the Rust book");
        assert_eq!(unfinished.project.as_deref(), Some("Reading"));
        assert!(unfinished.recurrence.is_some(), "the natural-language parser ran");
        assert!(snapshot.tasks.iter().any(|t| t.is_done()));
    }

    #[test]
    fn a_renamed_project_leaves_no_stale_file() {
        let temp = TempVault::new("prune");
        let vault = temp.vault();
        let mut store = Store::in_memory().unwrap();
        let mut task = Task::new("Something", today());
        task.project = Some("Old".into());
        store.save(&task).unwrap();
        vault.write(&store.snapshot().unwrap()).unwrap();
        assert!(temp.0.join(TASKS_DIR).join("old.md").exists());

        task.project = Some("New".into());
        store.save(&task).unwrap();
        vault.write(&store.snapshot().unwrap()).unwrap();

        assert!(temp.0.join(TASKS_DIR).join("new.md").exists());
        assert!(!temp.0.join(TASKS_DIR).join("old.md").exists(), "the old file is gone");
    }

    #[test]
    fn a_file_the_app_did_not_write_is_left_alone() {
        let temp = TempVault::new("foreign");
        let vault = temp.vault();
        std::fs::create_dir_all(temp.0.join(TASKS_DIR)).unwrap();
        let foreign = temp.0.join(TASKS_DIR).join("notes.txt");
        std::fs::write(&foreign, "not markdown, not ours").unwrap();

        vault.write(&populated().snapshot().unwrap()).unwrap();
        assert!(foreign.exists(), "pruning only ever removes .md files");
    }

    #[test]
    fn an_empty_folder_is_not_a_vault() {
        let temp = TempVault::new("empty");
        let vault = temp.vault();
        assert!(!vault.exists());
        vault.write(&populated().snapshot().unwrap()).unwrap();
        assert!(vault.exists());
    }

    #[test]
    fn a_broken_file_costs_only_that_file() {
        let temp = TempVault::new("broken");
        let vault = temp.vault();
        vault.write(&populated().snapshot().unwrap()).unwrap();
        std::fs::write(
            temp.0.join(TASKS_DIR).join("garbage.md"),
            "\u{fffd}\u{fffd} not really anything \u{0}",
        )
        .unwrap();

        let snapshot = vault.read(today()).unwrap();
        assert!(!snapshot.tasks.is_empty(), "the good files still read");
    }
}
