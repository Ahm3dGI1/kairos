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
    pub fn fingerprint(&self) -> std::io::Result<std::collections::BTreeMap<PathBuf, Vec<u8>>> {
        let mut files = std::collections::BTreeMap::new();
        for dir in [TASKS_DIR, HABITS_DIR, WORKOUTS_DIR] {
            for (path, text) in markdown_in(&self.root.join(dir))? {
                files.insert(
                    path.strip_prefix(&self.root).unwrap().to_path_buf(),
                    text.into_bytes(),
                );
            }
        }
        Ok(files)
    }

    pub fn write(&self, snapshot: &Snapshot) -> std::io::Result<bool> {
        let before = self.fingerprint()?;
        let after = self.write_checked(snapshot, &before)?;
        Ok(after != before)
    }

    pub fn write_checked(
        &self,
        snapshot: &Snapshot,
        before: &std::collections::BTreeMap<PathBuf, Vec<u8>>,
    ) -> std::io::Result<std::collections::BTreeMap<PathBuf, Vec<u8>>> {
        if self.fingerprint()? != *before {
            return Err(std::io::Error::other("Vault changed before save; reload and retry"));
        }
        let stage =
            Staging(std::env::temp_dir().join(format!("kairos-export-{}", uuid::Uuid::new_v4())));
        let staged = Vault::new(&stage.0);
        staged.write_inner(snapshot)?;
        let desired = staged.fingerprint()?;
        let mut changes = Vec::new();
        for (path, bytes) in &desired {
            if before.get(path) != Some(bytes) {
                changes.push((path.clone(), Some(bytes.clone())));
            }
        }
        for (path, bytes) in before {
            if !desired.contains_key(path) && managed_file(path, bytes) {
                changes.push((path.clone(), None));
            }
        }
        if changes.is_empty() {
            return Ok(before.clone());
        }
        let recovery = self.root.join(".kairos-backups").join(format!(
            "{}-{}",
            chrono::Utc::now().format("%Y%m%d%H%M%S%f"),
            uuid::Uuid::new_v4()
        ));
        for (path, bytes) in before {
            crate::io::atomic_write(&recovery.join(path), bytes)?;
        }
        let mut applied: Vec<(PathBuf, Option<Vec<u8>>)> = Vec::new();
        let result = (|| {
            for (path, bytes) in &changes {
                let full = self.root.join(path);
                let current = match std::fs::read(&full) {
                    Ok(bytes) => Some(bytes),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                    Err(e) => return Err(e),
                };
                if current.as_ref() != before.get(path) {
                    return Err(std::io::Error::other(
                        "An editor changed a file during save; reload and retry",
                    ));
                }
                if let Some(bytes) = bytes {
                    crate::io::atomic_write(&full, bytes)?;
                } else {
                    std::fs::remove_file(&full)?;
                }
                applied.push((path.clone(), bytes.clone()));
            }
            Ok(())
        })();
        if let Err(error) = result {
            for (path, written) in applied.iter().rev() {
                let full = self.root.join(path);
                // Never roll back over a newer edit made by another program.
                if std::fs::read(&full).ok() != *written {
                    continue;
                }
                if let Some(bytes) = before.get(path) {
                    let _ = crate::io::atomic_write(&full, bytes);
                } else {
                    let _ = std::fs::remove_file(&full);
                }
            }
            return Err(error);
        }
        let readme = self.root.join("README.md");
        if !readme.exists() {
            let _ = crate::io::atomic_write(&readme, README.as_bytes());
        }
        if let Ok(entries) = std::fs::read_dir(self.root.join(".kairos-backups")) {
            let mut paths: Vec<_> =
                entries.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.is_dir()).collect();
            paths.sort();
            for path in paths.iter().rev().skip(5) {
                let _ = std::fs::remove_dir_all(path);
            }
        }
        let mut written = before.clone();
        for (path, bytes) in changes {
            if let Some(bytes) = bytes {
                written.insert(path, bytes);
            } else {
                written.remove(&path);
            }
        }
        Ok(written)
    }

    fn write_inner(&self, snapshot: &Snapshot) -> std::io::Result<bool> {
        let mut changed = false;

        std::fs::create_dir_all(&self.root)?;
        changed |= write_if_changed(&self.root.join("README.md"), README)?;

        // -- tasks, one file per project
        let mut by_project: HashMap<Option<String>, Vec<crate::task::Task>> = HashMap::new();
        by_project.entry(None).or_default();
        for task in &snapshot.tasks {
            by_project.entry(task.project.clone()).or_default().push(task.clone());
        }

        let habit_names: tasks::HabitNames =
            snapshot.habits.iter().map(|h| (h.id, h.name.clone())).collect();

        let dir = self.root.join(TASKS_DIR);
        std::fs::create_dir_all(&dir)?;
        let mut expected = HashSet::new();
        let project_slugs: Vec<String> =
            by_project.keys().map(|p| tasks::slug(p.as_deref())).collect();
        for (project, mut group) in by_project {
            // A stable order, so the file does not reshuffle between writes.
            group.sort_by_key(|t| (t.due, t.created_at, t.id));
            let slug = tasks::slug(project.as_deref());
            let name = safe_name(
                &slug,
                project.as_deref().unwrap_or(""),
                project.is_some()
                    && (slug == "inbox"
                        || project_slugs.iter().filter(|s| **s == slug).count() > 1),
            );
            expected.insert(name.clone());
            changed |= write_if_changed(
                &dir.join(name),
                &tasks::write_file(project.as_deref(), &group, &habit_names),
            )?;
        }
        changed |= prune(&dir, &expected)?;

        // -- habits: the definitions, then a file per month
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

        // -- workouts, one file per routine
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

            let slug = workouts::slug(&routine.name);
            let collision =
                snapshot.routines.iter().filter(|r| workouts::slug(&r.name) == slug).count() > 1;
            let name = safe_name(&slug, &routine.id.to_string(), collision);
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
    pub fn read(&self, today: NaiveDate) -> std::io::Result<Snapshot> {
        let mut snapshot = Snapshot::default();

        // Habits first: a task may name one, and the name only resolves once
        // the definitions are in hand.
        self.read_habits_into(&mut snapshot, today)?;
        let by_name: tasks::HabitIds =
            snapshot.habits.iter().map(|h| (h.name.to_lowercase(), h.id)).collect();

        for (path, text) in markdown_in(&self.root.join(TASKS_DIR))? {
            let (heading, mut found) = tasks::read_file(&text, today, &by_name);
            let project = heading.or_else(|| {
                let stem = path.file_stem()?.to_string_lossy().to_string();
                (stem != "inbox").then_some(stem)
            });
            for task in &mut found {
                task.project = project.clone();
            }
            snapshot.tasks.append(&mut found);
        }

        for (index, (path, text)) in
            markdown_in(&self.root.join(WORKOUTS_DIR))?.into_iter().enumerate()
        {
            let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
            let mut file = workouts::read_routine(&text, &stem);
            file.routine.position = index as i64;
            snapshot.routines.push(file.routine);
            snapshot.exercises.append(&mut file.exercises);
            snapshot.sessions.append(&mut file.sessions);
        }

        Ok(snapshot)
    }

    /// Reads the habit definitions and every month file into `snapshot`.
    fn read_habits_into(&self, snapshot: &mut Snapshot, today: NaiveDate) -> std::io::Result<()> {
        let dir = self.root.join(HABITS_DIR);
        match std::fs::read_to_string(dir.join(HABITS_FILE)) {
            Ok(text) => snapshot.habits = habits::read_habits(&text, today),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        let mut known: HashMap<String, uuid::Uuid> =
            snapshot.habits.iter().map(|h| (h.name.to_lowercase(), h.id)).collect();

        for (path, text) in markdown_in(&dir)? {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            let Some((year, month)) = habits::month_of(&name) else { continue };
            let kinds = snapshot.habits.iter().map(|h| (h.id, h.kind.clone())).collect();
            let mut found = habits::read_month_with_kinds(&text, year, month, &known, &kinds);

            // A month file naming a habit no definition knew about creates it —
            // which is what makes `- Swim: 5 6 7` in a text editor enough.
            for (name, kind) in found.unknown_habits {
                let id = habits::derived_habit_id(&name);
                if known.values().any(|existing| *existing == id) {
                    continue;
                }
                known.insert(name.to_lowercase(), id);
                snapshot.habits.push(Habit {
                    id,
                    name,
                    kind,
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
        Ok(())
    }
}

struct Staging(PathBuf);
impl Drop for Staging {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn managed_file(path: &Path, bytes: &[u8]) -> bool {
    let text = String::from_utf8_lossy(bytes);
    match path.parent().and_then(|p| p.to_str()) {
        Some("tasks") => text.lines().any(|l| l.starts_with("- [")),
        Some("workouts") => {
            text.contains("<!-- kairos-ids:")
                || text.contains("## Exercises")
                || text.lines().any(|l| l.starts_with("## 20"))
        }
        Some("habits") => {
            path.file_name().and_then(|n| n.to_str()).is_some_and(|n| habits::month_of(n).is_some())
        }
        _ => false,
    }
}

fn safe_name(slug: &str, identity: &str, collision: bool) -> String {
    let reserved = matches!(slug, "con" | "prn" | "aux" | "nul")
        || (slug.len() == 4
            && (slug.starts_with("com") || slug.starts_with("lpt"))
            && slug.ends_with(['1', '2', '3', '4', '5', '6', '7', '8', '9']));
    if collision || reserved {
        format!(
            "{}-{}.md",
            slug,
            uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, identity.as_bytes())
        )
    } else {
        format!("{slug}.md")
    }
}

/// Writes only if the file does not already say exactly this.
fn write_if_changed(path: &Path, contents: &str) -> std::io::Result<bool> {
    if std::fs::read_to_string(path).is_ok_and(|existing| existing == contents) {
        return Ok(false);
    }
    crate::io::atomic_write(path, contents.as_bytes())?;
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
        let text = std::fs::read_to_string(&path)?;
        let managed = match dir.file_name().and_then(|s| s.to_str()) {
            Some("tasks") => text.lines().any(|l| l.starts_with("- [")),
            Some("workouts") => {
                text.contains("## Exercises") || text.lines().any(|l| l.starts_with("## 20"))
            }
            Some("habits") => habits::month_of(&name).is_some(),
            _ => false,
        };
        if !managed {
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
fn markdown_in(dir: &Path) -> std::io::Result<Vec<(PathBuf, String)>> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut files = Vec::new();
    for entry in entries {
        let path = entry?.path();
        if is_markdown(&path) {
            let text = std::fs::read_to_string(&path)?;
            files.push((path, text));
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(files)
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
retired. A habit is a tick unless it says otherwise:

    - Gym created:2026-01-05 ^<id>
    - Sleep kind:duration created:2026-01-05 ^<id>
    - Pages kind:number:pages created:2026-01-05 ^<id>

`2026-09.md` is one month. `## Ticks` lists the days each tick-habit was done;
`## Days` is a table with a column per numeric habit; then the month's
journal. Naming a habit in either is enough to create it — a column whose
first value reads as a time becomes a duration, and one holding a bare count
becomes a number.

A task can name a habit with `habit:Gym`, and completing that task ticks the
habit for the day.

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

        let mut sleep = store.add_habit("Sleep", today()).unwrap();
        sleep.kind = crate::daily::HabitKind::Duration;
        store.save_habit(&sleep).unwrap();

        let mut log = store.day_log(today()).unwrap();
        log.set_value(sleep.id, Some(450.0));
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

        assert_eq!(back.habits.len(), 2);
        let read = back.habits.iter().find(|h| h.name == "Read").unwrap();
        let sleep = back.habits.iter().find(|h| h.name == "Sleep").unwrap();
        assert_eq!(sleep.kind, crate::daily::HabitKind::Duration);
        assert_eq!(back.day_logs.len(), 1);
        assert_eq!(back.day_logs[0].value(sleep.id), Some(450.0));
        assert!(back.day_logs[0].is_done(read.id));

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
