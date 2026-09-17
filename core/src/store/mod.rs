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

use chrono::NaiveDate;
use rusqlite::{params, Connection, OptionalExtension};

use crate::filter::{Filter, Sort};
use crate::recur;
use crate::task::{Task, TaskId};

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

    /// Distinct tags in use, alphabetically.
    pub fn tags(&self) -> Result<Vec<String>> {
        let mut tags: Vec<String> =
            self.all()?.iter().filter(|t| !t.is_done()).flat_map(|t| t.tags.clone()).collect();
        tags.sort();
        tags.dedup();
        Ok(tags)
    }
}
