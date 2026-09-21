//! The local store: a SQLite database on the device.
//!
//! Local-first is the whole architecture, not a cache in front of a server, so
//! this is the system of record. Sync, when it lands, reconciles against it —
//! it never becomes the thing the app reads from directly.
//!
//! Every mutation records an undo entry, which is what makes deletions and
//! completions safe to do quickly. The log lives in the database, so undo still
//! works after a restart.

mod row;
mod snapshot;

use std::collections::{HashMap, HashSet};

use chrono::{Datelike, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension};

use crate::daily::{self, DayLog, Habit, HabitId, JournalEntry, MonthJournal};
use crate::filter::{Filter, Sort};
use crate::recur;
use crate::task::{Task, TaskId};
use crate::workout::{
    Exercise, ExerciseId, Routine, RoutineId, Session, SessionId, SessionLog, SetEntry,
};

/// Dates are stored ISO-8601 so they sort correctly as text in SQL.
const DATE_FORMAT: &str = "%Y-%m-%d";

/// How many undo entries to keep. Deep enough to recover from a mistaken burst
/// of completions, shallow enough that the log never becomes the database.
const UNDO_DEPTH: i64 = 200;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("could not read stored task: {0}")]
    Corrupt(#[from] serde_json::Error),
    #[error("no task with id {0}")]
    NotFound(TaskId),
}

type Result<T> = std::result::Result<T, StoreError>;

/// What an [`undo`](Store::undo) reversed, so a client can say so.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Undone {
    pub kind: UndoKind,
    /// The task as it stands after undoing, absent if undo removed it.
    pub task: Option<Task>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UndoKind {
    Create,
    Update,
    Complete,
    Delete,
}

impl UndoKind {
    fn as_str(self) -> &'static str {
        match self {
            UndoKind::Create => "create",
            UndoKind::Update => "update",
            UndoKind::Complete => "complete",
            UndoKind::Delete => "delete",
        }
    }

    fn parse(text: &str) -> Self {
        match text {
            "create" => UndoKind::Create,
            "complete" => UndoKind::Complete,
            "delete" => UndoKind::Delete,
            _ => UndoKind::Update,
        }
    }

    /// What to tell the user they are undoing.
    pub fn label(self) -> &'static str {
        match self {
            UndoKind::Create => "add",
            UndoKind::Update => "edit",
            UndoKind::Complete => "completion",
            UndoKind::Delete => "deletion",
        }
    }
}

pub use snapshot::Snapshot;

pub struct Store {
    conn: Connection,
}

impl Store {
    /// Opens (creating if needed) the database at `path`.
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self> {
        Self::from_connection(Connection::open(path)?)
    }

    /// An ephemeral database, for tests and for trying things out.
    pub fn in_memory() -> Result<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    /// Brings the schema up to date. Versioned through SQLite's own
    /// `user_version`, so a future migration is an added arm, never a rewrite.
    fn migrate(&self) -> Result<()> {
        let version: i64 =
            self.conn.query_row("PRAGMA user_version", [], |row| row.get(0)).unwrap_or(0);

        if version < 1 {
            self.conn.execute_batch(
                "CREATE TABLE tasks (
                     id           TEXT PRIMARY KEY,
                     title        TEXT NOT NULL,
                     notes        TEXT NOT NULL DEFAULT '',
                     due          TEXT,
                     time         TEXT,
                     recurrence   TEXT,
                     exceptions   TEXT NOT NULL DEFAULT '[]',
                     priority     TEXT NOT NULL DEFAULT 'none',
                     tags         TEXT NOT NULL DEFAULT '[]',
                     project      TEXT,
                     subtasks     TEXT NOT NULL DEFAULT '[]',
                     completed_at TEXT,
                     created_at   TEXT NOT NULL
                 );
                 CREATE INDEX tasks_due ON tasks (due);
                 CREATE INDEX tasks_project ON tasks (project);
                 CREATE INDEX tasks_completed ON tasks (completed_at);

                 CREATE TABLE undo_log (
                     seq     INTEGER PRIMARY KEY AUTOINCREMENT,
                     kind    TEXT NOT NULL,
                     task_id TEXT NOT NULL,
                     before  TEXT
                 );
                 PRAGMA user_version = 1;",
            )?;
        }

        // v2 adds the daily page: habits, and one row per day holding the
        // journal, the two hand-entered numbers, and which habits were ticked.
        if version < 2 {
            self.conn.execute_batch(
                "CREATE TABLE habits (
                     id         TEXT PRIMARY KEY,
                     name       TEXT NOT NULL,
                     created_at TEXT NOT NULL,
                     archived   INTEGER NOT NULL DEFAULT 0,
                     position   INTEGER NOT NULL DEFAULT 0
                 );

                 CREATE TABLE day_logs (
                     date           TEXT PRIMARY KEY,
                     journal        TEXT NOT NULL DEFAULT '',
                     screen_minutes INTEGER,
                     sleep_minutes  INTEGER,
                     habits_done    TEXT NOT NULL DEFAULT '[]'
                 );
                 PRAGMA user_version = 2;",
            )?;
        }

        // v3 moves the journal from one box per day to one page per month.
        // Writing about a week does not fit in a day-sized field.
        if version < 3 {
            self.conn.execute_batch(
                "CREATE TABLE month_journals (
                     month   TEXT PRIMARY KEY,
                     entries TEXT NOT NULL DEFAULT '[]'
                 );
                 PRAGMA user_version = 3;",
            )?;
            self.fold_day_journals_into_months()?;
        }

        // v4 gives undo a counterpart. Redo is the same log walked the other
        // way, so it is the same table twice rather than a new mechanism.
        if version < 4 {
            self.conn.execute_batch(
                "CREATE TABLE redo_log (
                     seq     INTEGER PRIMARY KEY AUTOINCREMENT,
                     kind    TEXT NOT NULL,
                     task_id TEXT NOT NULL,
                     before  TEXT
                 );
                 PRAGMA user_version = 4;",
            )?;
        }

        // v5: a task can say how its subtask list should be read.
        if version < 5 {
            self.conn.execute_batch(
                "ALTER TABLE tasks ADD COLUMN checklist TEXT NOT NULL DEFAULT 'all';
                 PRAGMA user_version = 5;",
            )?;
        }

        // v6: the workout book. Four tables that mirror the spreadsheet this
        // replaces — a page, its rows, a dated run of the page, and the sets.
        if version < 6 {
            self.conn.execute_batch(
                "CREATE TABLE routines (
                     id       TEXT PRIMARY KEY,
                     name     TEXT NOT NULL,
                     position INTEGER NOT NULL DEFAULT 0
                 );

                 CREATE TABLE exercises (
                     id       TEXT PRIMARY KEY,
                     routine  TEXT NOT NULL,
                     name     TEXT NOT NULL,
                     position INTEGER NOT NULL DEFAULT 0
                 );
                 CREATE INDEX exercises_routine ON exercises (routine);

                 CREATE TABLE sessions (
                     id      TEXT PRIMARY KEY,
                     routine TEXT NOT NULL,
                     date    TEXT NOT NULL,
                     note    TEXT NOT NULL DEFAULT ''
                 );
                 CREATE INDEX sessions_routine ON sessions (routine, date);

                 CREATE TABLE sets (
                     session  TEXT NOT NULL,
                     exercise TEXT NOT NULL,
                     idx      INTEGER NOT NULL,
                     reps     INTEGER NOT NULL DEFAULT 0,
                     weight   REAL NOT NULL DEFAULT 0,
                     PRIMARY KEY (session, exercise, idx)
                 );
                 PRAGMA user_version = 6;",
            )?;
        }

        // v7: a habit can be a number rather than a tick, and the two
        // hand-entered numbers stop being special. Screen time and sleep were
        // always habits of a kind the model could not yet express; they become
        // ordinary ones here, which is what lets any other number join them.
        if version < 7 {
            self.conn.execute_batch(
                "ALTER TABLE habits ADD COLUMN kind TEXT NOT NULL DEFAULT 'check';
                 ALTER TABLE day_logs ADD COLUMN numbers TEXT NOT NULL DEFAULT '{}';
                 ALTER TABLE tasks ADD COLUMN habit TEXT;
                 PRAGMA user_version = 7;",
            )?;
            self.lift_metrics_into_habits()?;
        }
        Ok(())
    }

    /// Turns the old `screen_minutes` and `sleep_minutes` columns into two
    /// ordinary duration habits, carrying every day's number across.
    ///
    /// Only creates a habit that some day actually used: a database that never
    /// recorded sleep should not grow a Sleep row for it.
    fn lift_metrics_into_habits(&self) -> Result<()> {
        let mut stmt = self.conn.prepare(
            "SELECT date, screen_minutes, sleep_minutes FROM day_logs
             WHERE screen_minutes IS NOT NULL OR sleep_minutes IS NOT NULL",
        )?;
        let rows: Vec<(String, Option<f64>, Option<f64>)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?;
        drop(stmt);
        if rows.is_empty() {
            return Ok(());
        }

        let mut position: i64 =
            self.conn.query_row("SELECT COALESCE(MAX(position), -1) + 1 FROM habits", [], |r| {
                r.get(0)
            })?;
        let mut make = |name: &str| -> Result<HabitId> {
            let id = uuid::Uuid::new_v4();
            self.conn.execute(
                "INSERT INTO habits (id, name, created_at, archived, position, kind)
                 VALUES (?1, ?2, ?3, 0, ?4, 'duration')",
                params![
                    id.to_string(),
                    name,
                    chrono::Local::now().date_naive().format(DATE_FORMAT).to_string(),
                    position
                ],
            )?;
            position += 1;
            Ok(id)
        };
        let screen = make("Screen")?;
        let sleep = make("Sleep")?;

        for (date, screen_minutes, sleep_minutes) in rows {
            let mut values = serde_json::Map::new();
            if let Some(minutes) = screen_minutes {
                values.insert(screen.to_string(), minutes.into());
            }
            if let Some(minutes) = sleep_minutes {
                values.insert(sleep.to_string(), minutes.into());
            }
            self.conn.execute(
                "UPDATE day_logs SET numbers = ?2 WHERE date = ?1",
                params![date, serde_json::Value::Object(values).to_string()],
            )?;
        }
        Ok(())
    }

    // ---------- the workout book ----------

    pub fn routines(&self) -> Result<Vec<Routine>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, position FROM routines ORDER BY position, name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, name, position) = row?;
            out.push(Routine { id: parse_id(&id), name, position });
        }
        Ok(out)
    }

    pub fn add_routine(&mut self, name: &str) -> Result<Routine> {
        let next: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(position), -1) + 1 FROM routines",
            [],
            |r| r.get(0),
        )?;
        let routine = Routine::new(name.trim(), next);
        self.save_routine(&routine)?;
        Ok(routine)
    }

    pub fn save_routine(&mut self, routine: &Routine) -> Result<()> {
        self.conn.execute(
            "INSERT INTO routines (id, name, position) VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET name = ?2, position = ?3",
            params![routine.id.to_string(), routine.name, routine.position],
        )?;
        Ok(())
    }

    /// Removes a routine and everything recorded under it.
    pub fn delete_routine(&mut self, id: RoutineId) -> Result<()> {
        let key = id.to_string();
        self.conn.execute(
            "DELETE FROM sets WHERE session IN (SELECT id FROM sessions WHERE routine = ?1)",
            params![key],
        )?;
        self.conn.execute("DELETE FROM sessions WHERE routine = ?1", params![key])?;
        self.conn.execute("DELETE FROM exercises WHERE routine = ?1", params![key])?;
        self.conn.execute("DELETE FROM routines WHERE id = ?1", params![key])?;
        Ok(())
    }

    pub fn exercises(&self, routine: RoutineId) -> Result<Vec<Exercise>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, position FROM exercises WHERE routine = ?1 ORDER BY position",
        )?;
        let rows = stmt.query_map(params![routine.to_string()], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, name, position) = row?;
            out.push(Exercise { id: parse_id(&id), routine, name, position });
        }
        Ok(out)
    }

    pub fn add_exercise(&mut self, routine: RoutineId, name: &str) -> Result<Exercise> {
        let next: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(position), -1) + 1 FROM exercises WHERE routine = ?1",
            params![routine.to_string()],
            |r| r.get(0),
        )?;
        let exercise = Exercise::new(routine, name.trim(), next);
        self.save_exercise(&exercise)?;
        Ok(exercise)
    }

    pub fn save_exercise(&mut self, exercise: &Exercise) -> Result<()> {
        self.conn.execute(
            "INSERT INTO exercises (id, routine, name, position) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET routine = ?2, name = ?3, position = ?4",
            params![
                exercise.id.to_string(),
                exercise.routine.to_string(),
                exercise.name,
                exercise.position
            ],
        )?;
        Ok(())
    }

    pub fn delete_exercise(&mut self, id: ExerciseId) -> Result<()> {
        self.conn.execute("DELETE FROM sets WHERE exercise = ?1", params![id.to_string()])?;
        self.conn.execute("DELETE FROM exercises WHERE id = ?1", params![id.to_string()])?;
        Ok(())
    }

    /// The most recent sessions of a routine, newest first.
    pub fn sessions(&self, routine: RoutineId, limit: u32) -> Result<Vec<SessionLog>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, date, note FROM sessions WHERE routine = ?1
             ORDER BY date DESC, rowid DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![routine.to_string(), limit], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
        })?;

        let mut out = Vec::new();
        for row in rows {
            let (id, date, note) = row?;
            let Ok(date) = NaiveDate::parse_from_str(&date, DATE_FORMAT) else { continue };
            let id = parse_id(&id);
            out.push(SessionLog {
                sets: self.sets(id)?,
                session: Session { id, routine, date, note },
            });
        }
        Ok(out)
    }

    pub fn sets(&self, session: SessionId) -> Result<Vec<SetEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT exercise, idx, reps, weight FROM sets WHERE session = ?1 ORDER BY idx",
        )?;
        let rows = stmt.query_map(params![session.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, u32>(1)?,
                row.get::<_, u32>(2)?,
                row.get::<_, f64>(3)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (exercise, index, reps, weight) = row?;
            out.push(SetEntry { exercise: parse_id(&exercise), index, reps, weight });
        }
        Ok(out)
    }

    /// Starts a session, carrying the previous one's numbers across.
    ///
    /// Copying last time is the difference between logging a workout and
    /// re-typing it: almost every set repeats, and the ones that change are the
    /// interesting ones.
    pub fn start_session(&mut self, routine: RoutineId, date: NaiveDate) -> Result<SessionLog> {
        let previous = self.sessions(routine, 1)?.into_iter().next();
        let session = Session::new(routine, date);
        self.conn.execute(
            "INSERT INTO sessions (id, routine, date, note) VALUES (?1, ?2, ?3, '')",
            params![
                session.id.to_string(),
                routine.to_string(),
                date.format(DATE_FORMAT).to_string()
            ],
        )?;

        let mut sets = Vec::new();
        if let Some(previous) = previous {
            for set in previous.sets {
                self.save_set(session.id, set)?;
                sets.push(set);
            }
        }
        Ok(SessionLog { session, sets })
    }

    pub fn save_set(&mut self, session: SessionId, set: SetEntry) -> Result<()> {
        self.conn.execute(
            "INSERT INTO sets (session, exercise, idx, reps, weight) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(session, exercise, idx) DO UPDATE SET reps = ?4, weight = ?5",
            params![session.to_string(), set.exercise.to_string(), set.index, set.reps, set.weight],
        )?;
        Ok(())
    }

    pub fn delete_set(
        &mut self,
        session: SessionId,
        exercise: ExerciseId,
        index: u32,
    ) -> Result<()> {
        self.conn.execute(
            "DELETE FROM sets WHERE session = ?1 AND exercise = ?2 AND idx = ?3",
            params![session.to_string(), exercise.to_string(), index],
        )?;
        Ok(())
    }

    pub fn save_session_note(&mut self, session: SessionId, note: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET note = ?2 WHERE id = ?1",
            params![session.to_string(), note],
        )?;
        Ok(())
    }

    pub fn delete_session(&mut self, session: SessionId) -> Result<()> {
        self.conn.execute("DELETE FROM sets WHERE session = ?1", params![session.to_string()])?;
        self.conn.execute("DELETE FROM sessions WHERE id = ?1", params![session.to_string()])?;
        Ok(())
    }

    /// Carries anything already written in the old per-day journals across into
    /// the month pages, so upgrading never silently drops what someone wrote.
    /// Runs during the v3 migration, so it reads and writes the columns that
    /// exist *at v3* rather than going through [`Store::all_day_logs`].
    ///
    /// A migration that calls the ordinary accessors is a migration that
    /// breaks the next time the schema moves: those queries describe today's
    /// table, and this one is running against an older one.
    fn fold_day_journals_into_months(&self) -> Result<()> {
        let mut stmt = self.conn.prepare("SELECT date, journal FROM day_logs")?;
        let rows: Vec<(String, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        drop(stmt);

        for (date, journal) in rows {
            if journal.trim().is_empty() {
                continue;
            }
            let Ok(parsed) = NaiveDate::parse_from_str(&date, DATE_FORMAT) else { continue };
            let key = MonthJournal::key(parsed.year(), parsed.month());
            let mut month = self.read_month_journal(&key, parsed.year(), parsed.month())?;

            let mut entry = JournalEntry::new(parsed.format("%A %-d").to_string());
            entry.body = journal;
            month.entries.push(entry);
            self.write_month_journal(&month)?;

            self.conn.execute("UPDATE day_logs SET journal = '' WHERE date = ?1", params![date])?;
        }
        Ok(())
    }

    /// Writes a task, inserting or replacing. Records an undo entry describing
    /// whatever it displaced.
    pub fn save(&mut self, task: &Task) -> Result<()> {
        let before = self.get(task.id)?;
        let kind = if before.is_some() { UndoKind::Update } else { UndoKind::Create };
        self.record_undo(kind, task.id, before.as_ref())?;
        self.write(task)
    }

    /// Writes a task without touching the undo log — used when restoring.
    fn write(&self, task: &Task) -> Result<()> {
        let r = row::Row::from_task(task)?;
        self.conn.execute(
            "INSERT INTO tasks
                 (id, title, notes, due, time, recurrence, exceptions,
                  priority, tags, project, subtasks, completed_at, created_at,
                  checklist, habit)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
             ON CONFLICT(id) DO UPDATE SET
                 title = ?2, notes = ?3, due = ?4, time = ?5, recurrence = ?6,
                 exceptions = ?7, priority = ?8, tags = ?9, project = ?10,
                 subtasks = ?11, completed_at = ?12, created_at = ?13,
                 checklist = ?14, habit = ?15",
            params![
                r.id,
                r.title,
                r.notes,
                r.due,
                r.time,
                r.recurrence,
                r.exceptions,
                r.priority,
                r.tags,
                r.project,
                r.subtasks,
                r.completed_at,
                r.created_at,
                r.checklist,
                r.habit,
            ],
        )?;
        Ok(())
    }

    pub fn get(&self, id: TaskId) -> Result<Option<Task>> {
        let found = self
            .conn
            .query_row("SELECT * FROM tasks WHERE id = ?1", params![id.to_string()], |row| {
                row::Row::from_sql(row)
            })
            .optional()?;
        found.map(|r| r.into_task()).transpose()
    }

    /// Every task, completed ones included, in no particular order.
    pub fn all(&self) -> Result<Vec<Task>> {
        let mut stmt = self.conn.prepare("SELECT * FROM tasks")?;
        let rows = stmt.query_map([], row::Row::from_sql)?;
        rows.map(|r| r?.into_task()).collect()
    }

    /// The tasks a view shows, ordered.
    ///
    /// Filtering happens in Rust rather than SQL: recurrence expansion decides
    /// membership for half these views, and that logic belongs in the core
    /// where every client shares it, not duplicated into a query.
    pub fn query(&self, filter: &Filter, sort: Sort, today: NaiveDate) -> Result<Vec<Task>> {
        let mut tasks: Vec<Task> =
            self.all()?.into_iter().filter(|t| filter.matches(t, today)).collect();
        sort.apply(&mut tasks, today);
        Ok(tasks)
    }

    /// Completes one occurrence. A recurring task advances to its next
    /// occurrence instead of closing — see [`recur::complete_occurrence`].
    pub fn complete(&mut self, id: TaskId, today: NaiveDate) -> Result<Task> {
        let mut task = self.get(id)?.ok_or(StoreError::NotFound(id))?;
        self.record_undo(UndoKind::Complete, id, Some(&task))?;
        recur::complete_occurrence(&mut task, today);
        self.write(&task)?;

        // A task standing for a habit records the habit too. Ticking rather
        // than toggling: completing the task twice in a day is one day done,
        // not a day undone.
        if let Some(habit) = task.habit {
            let mut log = self.day_log(today)?;
            if !log.is_done(habit) {
                log.habits_done.push(habit);
                self.save_day_log(&log)?;
            }
        }
        Ok(task)
    }

    /// Reopens a completed task.
    pub fn uncomplete(&mut self, id: TaskId) -> Result<Task> {
        let mut task = self.get(id)?.ok_or(StoreError::NotFound(id))?;
        self.record_undo(UndoKind::Update, id, Some(&task))?;
        task.completed_at = None;
        self.write(&task)?;
        Ok(task)
    }

    pub fn delete(&mut self, id: TaskId) -> Result<()> {
        let task = self.get(id)?.ok_or(StoreError::NotFound(id))?;
        self.record_undo(UndoKind::Delete, id, Some(&task))?;
        self.conn.execute("DELETE FROM tasks WHERE id = ?1", params![id.to_string()])?;
        Ok(())
    }

    /// Reverses the most recent mutation, and remembers it so redo can put it
    /// back. Returns `None` when there is nothing left to undo.
    pub fn undo(&mut self) -> Result<Option<Undone>> {
        self.step("undo_log", "redo_log")
    }

    /// Replays the last thing undone. Cleared as soon as anything else is
    /// written, because a redo onto a changed world is not the same act.
    pub fn redo(&mut self) -> Result<Option<Undone>> {
        self.step("redo_log", "undo_log")
    }

    /// Pops the newest entry off `from`, pushes the state it is about to
    /// replace onto `onto`, and applies it. Undo and redo are the same walk in
    /// opposite directions, so they are the same code.
    fn step(&mut self, from: &str, onto: &str) -> Result<Option<Undone>> {
        let entry: Option<(i64, String, String, Option<String>)> = self
            .conn
            .query_row(
                &format!("SELECT seq, kind, task_id, before FROM {from} ORDER BY seq DESC LIMIT 1"),
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;

        let Some((seq, kind, task_id, before)) = entry else { return Ok(None) };
        self.conn.execute(&format!("DELETE FROM {from} WHERE seq = ?1"), params![seq])?;

        // What the world looks like now becomes the other log's entry.
        let id = uuid::Uuid::parse_str(&task_id).ok();
        let current = id.and_then(|id| self.get(id).ok().flatten());
        self.push(onto, &kind, &task_id, current.as_ref())?;

        let kind = UndoKind::parse(&kind);
        let task = match before {
            // There was a prior state: put it back.
            Some(json) => {
                let task: Task = serde_json::from_str(&json)?;
                self.write(&task)?;
                Some(task)
            }
            // There was none, so the mutation created the task: remove it.
            None => {
                self.conn.execute("DELETE FROM tasks WHERE id = ?1", params![task_id])?;
                None
            }
        };
        Ok(Some(Undone { kind, task }))
    }

    /// Whether anything can be undone, for enabling the menu item.
    pub fn can_undo(&self) -> Result<bool> {
        self.log_has("undo_log")
    }

    pub fn can_redo(&self) -> Result<bool> {
        self.log_has("redo_log")
    }

    fn log_has(&self, table: &str) -> Result<bool> {
        let count: i64 =
            self.conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))?;
        Ok(count > 0)
    }

    fn record_undo(&self, kind: UndoKind, id: TaskId, before: Option<&Task>) -> Result<()> {
        self.push("undo_log", kind.as_str(), &id.to_string(), before)?;
        // A fresh edit makes any redo meaningless: it would replay onto a world
        // that has moved on.
        self.conn.execute("DELETE FROM redo_log", [])?;
        Ok(())
    }

    fn push(&self, table: &str, kind: &str, id: &str, state: Option<&Task>) -> Result<()> {
        let json = state.map(serde_json::to_string).transpose()?;
        self.conn.execute(
            &format!("INSERT INTO {table} (kind, task_id, before) VALUES (?1, ?2, ?3)"),
            params![kind, id, json],
        )?;
        // Trim the oldest entries past the depth limit.
        self.conn.execute(
            &format!("DELETE FROM {table} WHERE seq <= (SELECT MAX(seq) FROM {table}) - ?1"),
            params![UNDO_DEPTH],
        )?;
        Ok(())
    }

    /// Distinct project names in use, alphabetically.
    pub fn projects(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT project FROM tasks
             WHERE project IS NOT NULL AND completed_at IS NULL
             ORDER BY project COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    // ---------- habits and the daily log ----------

    /// Habits in display order. Archived ones are left out unless asked for.
    pub fn habits(&self, include_archived: bool) -> Result<Vec<Habit>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, created_at, archived, position, kind FROM habits
             WHERE (?1 OR archived = 0)
             ORDER BY position, name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map(params![include_archived], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, bool>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?;

        let mut habits = Vec::new();
        for row in rows {
            let (id, name, created_at, archived, position, kind) = row?;
            habits.push(Habit {
                id: uuid::Uuid::parse_str(&id).unwrap_or_else(|_| uuid::Uuid::new_v4()),
                name,
                kind: crate::daily::HabitKind::parse(&kind),
                created_at: NaiveDate::parse_from_str(&created_at, DATE_FORMAT)
                    .unwrap_or_else(|_| chrono::Local::now().date_naive()),
                archived,
                position,
            });
        }
        Ok(habits)
    }

    /// Adds a habit at the end of the list.
    pub fn add_habit(&mut self, name: &str, today: NaiveDate) -> Result<Habit> {
        let next: i64 =
            self.conn.query_row("SELECT COALESCE(MAX(position), -1) + 1 FROM habits", [], |r| {
                r.get(0)
            })?;
        let habit = Habit::new(name.trim(), today, next);
        self.save_habit(&habit)?;
        Ok(habit)
    }

    pub fn save_habit(&mut self, habit: &Habit) -> Result<()> {
        self.conn.execute(
            "INSERT INTO habits (id, name, created_at, archived, position, kind)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
                 name = ?2, created_at = ?3, archived = ?4, position = ?5, kind = ?6",
            params![
                habit.id.to_string(),
                habit.name,
                habit.created_at.format(DATE_FORMAT).to_string(),
                habit.archived,
                habit.position,
                habit.kind.label(),
            ],
        )?;
        Ok(())
    }

    /// Removes a habit and every tick of it. Archiving is the gentler option.
    pub fn delete_habit(&mut self, id: HabitId) -> Result<()> {
        self.conn.execute("DELETE FROM habits WHERE id = ?1", params![id.to_string()])?;

        // Ticks live inside each day's row, so they have to be pruned by hand.
        // Every row, not a date range: dates are stored as text, and the
        // NaiveDate::MIN/MAX sentinels format with leading "-" and "+", which
        // sort either side of ordinary years and make BETWEEN match nothing.
        let logs = self.all_day_logs()?;
        for mut log in logs {
            if log.habits_done.contains(&id) {
                log.habits_done.retain(|h| *h != id);
                self.save_day_log(&log)?;
            }
        }
        Ok(())
    }

    /// The day's row, or an empty one if nothing has been recorded yet.
    pub fn day_log(&self, date: NaiveDate) -> Result<DayLog> {
        let found = self
            .conn
            .query_row(
                "SELECT journal, habits_done, numbers FROM day_logs WHERE date = ?1",
                params![date.format(DATE_FORMAT).to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?;

        let Some((journal, habits_done, values)) = found else {
            return Ok(DayLog::new(date));
        };
        Ok(DayLog {
            date,
            journal,
            habits_done: serde_json::from_str(&habits_done)?,
            values: serde_json::from_str(&values)?,
        })
    }

    /// Writes the day's row, or removes it once nothing is left on it.
    pub fn save_day_log(&mut self, log: &DayLog) -> Result<()> {
        self.write_day_log(log)
    }

    fn write_day_log(&self, log: &DayLog) -> Result<()> {
        let date = log.date.format(DATE_FORMAT).to_string();
        if log.is_empty() {
            self.conn.execute("DELETE FROM day_logs WHERE date = ?1", params![date])?;
            return Ok(());
        }
        self.conn.execute(
            "INSERT INTO day_logs (date, journal, habits_done, numbers)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(date) DO UPDATE SET journal = ?2, habits_done = ?3, numbers = ?4",
            params![
                date,
                log.journal,
                serde_json::to_string(&log.habits_done)?,
                serde_json::to_string(&log.values)?,
            ],
        )?;
        Ok(())
    }

    /// Ticks or un-ticks a habit on a day, returning its new state.
    pub fn toggle_habit(&mut self, habit: HabitId, date: NaiveDate) -> Result<bool> {
        let mut log = self.day_log(date)?;
        let now_done = log.toggle(habit);
        self.save_day_log(&log)?;
        Ok(now_done)
    }

    pub fn day_logs_between(&self, from: NaiveDate, to: NaiveDate) -> Result<Vec<DayLog>> {
        let mut stmt = self.conn.prepare(
            "SELECT date, journal, habits_done, numbers
             FROM day_logs WHERE date BETWEEN ?1 AND ?2 ORDER BY date",
        )?;
        let rows = stmt.query_map(
            params![from.format(DATE_FORMAT).to_string(), to.format(DATE_FORMAT).to_string()],
            day_log_columns,
        )?;
        collect_day_logs(rows)
    }

    /// Every recorded day, for the rare operation that has to touch all of them.
    pub(crate) fn all_day_logs(&self) -> Result<Vec<DayLog>> {
        let mut stmt = self
            .conn
            .prepare("SELECT date, journal, habits_done, numbers FROM day_logs ORDER BY date")?;
        let rows = stmt.query_map([], day_log_columns)?;
        collect_day_logs(rows)
    }

    /// The current run of days for every habit, computed in one pass over the
    /// recent history rather than one query per habit.
    pub fn habit_streaks(&self, today: NaiveDate) -> Result<HashMap<HabitId, u32>> {
        // A year of history bounds any streak worth displaying.
        let from = today - chrono::Duration::days(366);
        let logs = self.day_logs_between(from, today)?;

        let mut days: HashMap<HabitId, HashSet<NaiveDate>> = HashMap::new();
        for log in logs {
            for habit in log.habits_done {
                days.entry(habit).or_default().insert(log.date);
            }
        }
        Ok(days.into_iter().map(|(id, done)| (id, daily::streak(&done, today))).collect())
    }

    // ---------- the month journal ----------

    /// A month's journal page, empty if nothing has been written in it.
    pub fn month_journal(&self, year: i32, month: u32) -> Result<MonthJournal> {
        self.read_month_journal(&MonthJournal::key(year, month), year, month)
    }

    fn read_month_journal(&self, key: &str, year: i32, month: u32) -> Result<MonthJournal> {
        let found = self
            .conn
            .query_row("SELECT entries FROM month_journals WHERE month = ?1", params![key], |row| {
                row.get::<_, String>(0)
            })
            .optional()?;

        Ok(match found {
            Some(entries) => MonthJournal { year, month, entries: serde_json::from_str(&entries)? },
            None => MonthJournal::new(year, month),
        })
    }

    /// Writes a month page, or removes it once every entry has been emptied.
    pub fn save_month_journal(&mut self, journal: &MonthJournal) -> Result<()> {
        self.write_month_journal(journal)
    }

    fn write_month_journal(&self, journal: &MonthJournal) -> Result<()> {
        let key = MonthJournal::key(journal.year, journal.month);
        if journal.is_empty() {
            self.conn.execute("DELETE FROM month_journals WHERE month = ?1", params![key])?;
            return Ok(());
        }
        self.conn.execute(
            "INSERT INTO month_journals (month, entries) VALUES (?1, ?2)
             ON CONFLICT(month) DO UPDATE SET entries = ?2",
            params![key, serde_json::to_string(&journal.entries)?],
        )?;
        Ok(())
    }

    /// Distinct tags in use, alphabetically.
    pub fn tags(&self) -> Result<Vec<String>> {
        let mut tags: Vec<String> =
            self.all()?.iter().filter(|t| !t.is_done()).flat_map(|t| t.tags.clone()).collect();
        tags.sort();
        tags.dedup();
        Ok(tags)
    }
}

/// The raw columns of a `day_logs` row, in select order.
type DayLogColumns = (String, String, String, String);

fn day_log_columns(row: &rusqlite::Row<'_>) -> rusqlite::Result<DayLogColumns> {
    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
}

fn collect_day_logs(
    rows: impl Iterator<Item = rusqlite::Result<DayLogColumns>>,
) -> Result<Vec<DayLog>> {
    let mut logs = Vec::new();
    for row in rows {
        let (date, journal, habits_done, values) = row?;
        // A row whose date will not parse is unreadable rather than wrong;
        // skipping it keeps the rest of the history usable.
        let Ok(date) = NaiveDate::parse_from_str(&date, DATE_FORMAT) else { continue };
        logs.push(DayLog {
            date,
            journal,
            habits_done: serde_json::from_str(&habits_done)?,
            values: serde_json::from_str(&values)?,
        });
    }
    Ok(logs)
}

/// A stored id that will not parse is unreadable rather than wrong; a fresh one
/// keeps the row usable instead of failing the whole query.
fn parse_id(text: &str) -> uuid::Uuid {
    uuid::Uuid::parse_str(text).unwrap_or_else(|_| uuid::Uuid::new_v4())
}
