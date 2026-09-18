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

use std::collections::{HashMap, HashSet};

use chrono::{Datelike, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension};

use crate::daily::{self, DayLog, Habit, HabitId, JournalEntry, MonthJournal};
use crate::filter::{Filter, Sort};
use crate::recur;
use crate::task::{Task, TaskId};

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
        Ok(())
    }

    /// Carries anything already written in the old per-day journals across into
    /// the month pages, so upgrading never silently drops what someone wrote.
    fn fold_day_journals_into_months(&self) -> Result<()> {
        for log in self.all_day_logs()? {
            if log.journal.trim().is_empty() {
                continue;
            }
            let key = MonthJournal::key(log.date.year(), log.date.month());
            let mut journal = self.read_month_journal(&key, log.date.year(), log.date.month())?;

            let mut entry = JournalEntry::new(log.date.format("%A %-d").to_string());
            entry.body = log.journal.clone();
            journal.entries.push(entry);
            self.write_month_journal(&journal)?;

            let mut cleared = log.clone();
            cleared.journal.clear();
            // Written directly: save_day_log would drop a row that is now empty,
            // which is exactly right here.
            self.write_day_log(&cleared)?;
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
                  priority, tags, project, subtasks, completed_at, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(id) DO UPDATE SET
                 title = ?2, notes = ?3, due = ?4, time = ?5, recurrence = ?6,
                 exceptions = ?7, priority = ?8, tags = ?9, project = ?10,
                 subtasks = ?11, completed_at = ?12, created_at = ?13",
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

    /// Reverses the most recent mutation. Returns `None` when there is nothing
    /// left to undo.
    pub fn undo(&mut self) -> Result<Option<Undone>> {
        let entry: Option<(i64, String, String, Option<String>)> = self
            .conn
            .query_row(
                "SELECT seq, kind, task_id, before FROM undo_log ORDER BY seq DESC LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;

        let Some((seq, kind, task_id, before)) = entry else { return Ok(None) };
        self.conn.execute("DELETE FROM undo_log WHERE seq = ?1", params![seq])?;

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
        let count: i64 = self.conn.query_row("SELECT COUNT(*) FROM undo_log", [], |r| r.get(0))?;
        Ok(count > 0)
    }

    fn record_undo(&self, kind: UndoKind, id: TaskId, before: Option<&Task>) -> Result<()> {
        let json = before.map(serde_json::to_string).transpose()?;
        self.conn.execute(
            "INSERT INTO undo_log (kind, task_id, before) VALUES (?1, ?2, ?3)",
            params![kind.as_str(), id.to_string(), json],
        )?;
        // Trim the oldest entries past the depth limit.
        self.conn.execute(
            "DELETE FROM undo_log WHERE seq <= (
                 SELECT MAX(seq) FROM undo_log
             ) - ?1",
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
            "SELECT id, name, created_at, archived, position FROM habits
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
            ))
        })?;

        let mut habits = Vec::new();
        for row in rows {
            let (id, name, created_at, archived, position) = row?;
            habits.push(Habit {
                id: uuid::Uuid::parse_str(&id).unwrap_or_else(|_| uuid::Uuid::new_v4()),
                name,
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
            "INSERT INTO habits (id, name, created_at, archived, position)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
                 name = ?2, created_at = ?3, archived = ?4, position = ?5",
            params![
                habit.id.to_string(),
                habit.name,
                habit.created_at.format(DATE_FORMAT).to_string(),
                habit.archived,
                habit.position,
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
                "SELECT journal, screen_minutes, sleep_minutes, habits_done
                 FROM day_logs WHERE date = ?1",
                params![date.format(DATE_FORMAT).to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<u32>>(1)?,
                        row.get::<_, Option<u32>>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()?;

        let Some((journal, screen_minutes, sleep_minutes, habits_done)) = found else {
            return Ok(DayLog::new(date));
        };
        Ok(DayLog {
            date,
            journal,
            screen_minutes,
            sleep_minutes,
            habits_done: serde_json::from_str(&habits_done)?,
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
            "INSERT INTO day_logs (date, journal, screen_minutes, sleep_minutes, habits_done)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(date) DO UPDATE SET
                 journal = ?2, screen_minutes = ?3, sleep_minutes = ?4, habits_done = ?5",
            params![
                date,
                log.journal,
                log.screen_minutes,
                log.sleep_minutes,
                serde_json::to_string(&log.habits_done)?,
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
            "SELECT date, journal, screen_minutes, sleep_minutes, habits_done
             FROM day_logs WHERE date BETWEEN ?1 AND ?2 ORDER BY date",
        )?;
        let rows = stmt.query_map(
            params![from.format(DATE_FORMAT).to_string(), to.format(DATE_FORMAT).to_string()],
            day_log_columns,
        )?;
        collect_day_logs(rows)
    }

    /// Every recorded day, for the rare operation that has to touch all of them.
    fn all_day_logs(&self) -> Result<Vec<DayLog>> {
        let mut stmt = self.conn.prepare(
            "SELECT date, journal, screen_minutes, sleep_minutes, habits_done
             FROM day_logs ORDER BY date",
        )?;
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
type DayLogColumns = (String, String, Option<u32>, Option<u32>, String);

fn day_log_columns(row: &rusqlite::Row<'_>) -> rusqlite::Result<DayLogColumns> {
    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))
}

fn collect_day_logs(
    rows: impl Iterator<Item = rusqlite::Result<DayLogColumns>>,
) -> Result<Vec<DayLog>> {
    let mut logs = Vec::new();
    for row in rows {
        let (date, journal, screen_minutes, sleep_minutes, habits_done) = row?;
        // A row whose date will not parse is unreadable rather than wrong;
        // skipping it keeps the rest of the history usable.
        let Ok(date) = NaiveDate::parse_from_str(&date, DATE_FORMAT) else { continue };
        logs.push(DayLog {
            date,
            journal,
            screen_minutes,
            sleep_minutes,
            habits_done: serde_json::from_str(&habits_done)?,
        });
    }
    Ok(logs)
}
