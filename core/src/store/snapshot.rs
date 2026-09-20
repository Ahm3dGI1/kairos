//! The whole database as plain values, and the two conversions either side of
//! it.
//!
//! This is the seam that lets the vault be the system of record without the
//! store knowing anything about files, or the vault anything about SQL. The
//! store can produce a [`Snapshot`] and be rebuilt from one; the vault can
//! write a snapshot out as Markdown and read it back. Neither imports the
//! other.

use chrono::NaiveDate;
use rusqlite::params;

use super::{Result, Store, DATE_FORMAT};
use crate::daily::{DayLog, Habit, MonthJournal};
use crate::task::Task;
use crate::workout::{Exercise, Routine, SessionLog};

/// Everything the app stores, except the undo and redo logs.
///
/// Those stay out on purpose: they describe a session's history of edits, not
/// the user's data, and replaying them against a world rebuilt from files would
/// be undoing onto something that never happened.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Snapshot {
    pub tasks: Vec<Task>,
    pub habits: Vec<Habit>,
    pub day_logs: Vec<DayLog>,
    pub journals: Vec<MonthJournal>,
    pub routines: Vec<Routine>,
    pub exercises: Vec<Exercise>,
    pub sessions: Vec<SessionLog>,
}

impl Snapshot {
    /// Whether there is nothing here at all — which is how a first run decides
    /// to seed the vault from the database rather than the other way round.
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
            && self.habits.is_empty()
            && self.day_logs.is_empty()
            && self.journals.is_empty()
            && self.routines.is_empty()
    }
}

impl Store {
    /// Reads everything out.
    pub fn snapshot(&self) -> Result<Snapshot> {
        let routines = self.routines()?;
        let mut exercises = Vec::new();
        let mut sessions = Vec::new();
        for routine in &routines {
            exercises.extend(self.exercises(routine.id)?);
            sessions.extend(self.sessions(routine.id, u32::MAX)?);
        }

        Ok(Snapshot {
            tasks: self.all()?,
            habits: self.habits(true)?,
            day_logs: self.all_day_logs()?,
            journals: self.all_month_journals()?,
            routines,
            exercises,
            sessions,
        })
    }

    /// Replaces everything with `snapshot`, in one transaction.
    ///
    /// Nothing here touches the undo log: an import is not an edit the user
    /// made, and offering to undo it would mean offering to undo whatever they
    /// just typed in their editor.
    pub fn restore(&mut self, snapshot: &Snapshot) -> Result<()> {
        self.conn.execute_batch("BEGIN IMMEDIATE")?;
        match self.restore_inner(snapshot) {
            Ok(()) => {
                self.conn.execute_batch("COMMIT")?;
                Ok(())
            }
            Err(error) => {
                // A half-applied import would be worse than a failed one.
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(error)
            }
        }
    }

    fn restore_inner(&self, snapshot: &Snapshot) -> Result<()> {
        self.conn.execute_batch(
            "DELETE FROM sets;
             DELETE FROM sessions;
             DELETE FROM exercises;
             DELETE FROM routines;
             DELETE FROM month_journals;
             DELETE FROM day_logs;
             DELETE FROM habits;
             DELETE FROM tasks;",
        )?;

        for task in &snapshot.tasks {
            self.write(task)?;
        }
        for habit in &snapshot.habits {
            self.conn.execute(
                "INSERT INTO habits (id, name, created_at, archived, position)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    habit.id.to_string(),
                    habit.name,
                    habit.created_at.format(DATE_FORMAT).to_string(),
                    habit.archived,
                    habit.position,
                ],
            )?;
        }
        for log in &snapshot.day_logs {
            self.write_day_log(log)?;
        }
        for journal in &snapshot.journals {
            self.write_month_journal(journal)?;
        }
        for routine in &snapshot.routines {
            self.conn.execute(
                "INSERT INTO routines (id, name, position) VALUES (?1, ?2, ?3)",
                params![routine.id.to_string(), routine.name, routine.position],
            )?;
        }
        for exercise in &snapshot.exercises {
            self.conn.execute(
                "INSERT INTO exercises (id, routine, name, position) VALUES (?1, ?2, ?3, ?4)",
                params![
                    exercise.id.to_string(),
                    exercise.routine.to_string(),
                    exercise.name,
                    exercise.position,
                ],
            )?;
        }
        for log in &snapshot.sessions {
            self.conn.execute(
                "INSERT INTO sessions (id, routine, date, note) VALUES (?1, ?2, ?3, ?4)",
                params![
                    log.session.id.to_string(),
                    log.session.routine.to_string(),
                    log.session.date.format(DATE_FORMAT).to_string(),
                    log.session.note,
                ],
            )?;
            for set in &log.sets {
                self.conn.execute(
                    "INSERT INTO sets (session, exercise, idx, reps, weight)
                     VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(session, exercise, idx) DO UPDATE SET reps = ?4, weight = ?5",
                    params![
                        log.session.id.to_string(),
                        set.exercise.to_string(),
                        set.index,
                        set.reps,
                        set.weight,
                    ],
                )?;
            }
        }
        Ok(())
    }

    pub(crate) fn all_month_journals(&self) -> Result<Vec<MonthJournal>> {
        let mut stmt =
            self.conn.prepare("SELECT month, entries FROM month_journals ORDER BY month")?;
        let rows =
            stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;

        let mut out = Vec::new();
        for row in rows {
            let (key, entries) = row?;
            let Some((year, month)) = key.split_once('-') else { continue };
            let (Ok(year), Ok(month)) = (year.parse::<i32>(), month.parse::<u32>()) else {
                continue;
            };
            let entries = serde_json::from_str(&entries)?;
            out.push(MonthJournal { year, month, entries });
        }
        Ok(out)
    }

    /// The date of the newest thing in the store, for deciding whether a vault
    /// or a database is the more recent of the two.
    pub fn latest_change(&self) -> Result<Option<NaiveDate>> {
        let newest: Option<String> = self.conn.query_row(
            "SELECT MAX(d) FROM (
                 SELECT MAX(created_at) AS d FROM tasks
                 UNION ALL SELECT MAX(completed_at) FROM tasks
                 UNION ALL SELECT MAX(date) FROM day_logs
                 UNION ALL SELECT MAX(date) FROM sessions
             )",
            [],
            |row| row.get(0),
        )?;
        Ok(newest.and_then(|d| NaiveDate::parse_from_str(&d, DATE_FORMAT).ok()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::Task;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, 20).unwrap()
    }

    fn populated() -> Store {
        let mut store = Store::in_memory().unwrap();
        let mut task = Task::new("Gym", today());
        task.due = Some(today());
        task.add_tag("health");
        store.save(&task).unwrap();

        let habit = store.add_habit("Read", today()).unwrap();
        store.toggle_habit(habit.id, today()).unwrap();

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
    fn a_snapshot_restores_into_an_identical_store() {
        let original = populated();
        let snapshot = original.snapshot().unwrap();

        let mut rebuilt = Store::in_memory().unwrap();
        rebuilt.restore(&snapshot).unwrap();

        assert_eq!(rebuilt.snapshot().unwrap(), snapshot);
    }

    #[test]
    fn restoring_replaces_rather_than_merges() {
        let mut store = populated();
        let before = store.snapshot().unwrap();
        assert!(!before.tasks.is_empty());

        store.restore(&Snapshot::default()).unwrap();
        let after = store.snapshot().unwrap();
        assert!(after.is_empty(), "everything the snapshot omitted is gone");
    }

    #[test]
    fn an_import_does_not_become_an_undo_step() {
        let mut store = populated();
        let snapshot = store.snapshot().unwrap();
        // Drain whatever populating the store logged.
        while store.undo().unwrap().is_some() {}
        assert!(!store.can_undo().unwrap());

        store.restore(&snapshot).unwrap();
        assert!(!store.can_undo().unwrap(), "restoring is not an edit");
    }

    #[test]
    fn journals_survive_the_trip() {
        let mut store = Store::in_memory().unwrap();
        let mut journal = MonthJournal::new(2026, 9);
        let mut entry = crate::daily::JournalEntry::new("Week one");
        entry.body = "Something happened.".into();
        journal.entries.push(entry);
        store.save_month_journal(&journal).unwrap();

        let snapshot = store.snapshot().unwrap();
        assert_eq!(snapshot.journals.len(), 1);

        let mut rebuilt = Store::in_memory().unwrap();
        rebuilt.restore(&snapshot).unwrap();
        assert_eq!(rebuilt.month_journal(2026, 9).unwrap(), journal);
    }
}
