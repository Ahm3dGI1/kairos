//! Shared business logic for Kairos.
//!
//! Everything a client needs to know about what a task *is* lives here: the
//! model, the natural-language parsing that turns a single typed line into one,
//! the recurrence engine, and the local store. No UI, no platform APIs — this
//! crate compiles the same for the Windows shell, a Linux TUI, and (later, over
//! FFI) mobile.
//!
//! ```
//! use chrono::{NaiveDate, NaiveTime};
//! use kairos_core::{parse_at, Recurrence};
//!
//! let now = NaiveDate::from_ymd_opt(2026, 9, 13).unwrap().and_hms_opt(9, 0, 0).unwrap();
//! let result = parse_at("gym every day 5pm #health", now);
//!
//! assert_eq!(result.task.title, "gym");
//! assert_eq!(result.task.recurrence, Some(Recurrence::Daily));
//! assert_eq!(result.task.time, NaiveTime::from_hms_opt(17, 0, 0));
//! assert_eq!(result.task.tags, ["health"]);
//! ```
//!
//! Times and dates are naive local values: the core does not carry a timezone,
//! and the shell supplies one. That holds until multi-device sync forces the
//! question — see `docs/todo-app-spec.md` §6.

pub mod agenda;
pub mod daily;
pub mod filter;
pub mod parse;
pub mod recur;
pub mod settings;
pub mod store;
pub mod task;
pub mod vault;
pub mod workout;

pub use agenda::{Bucket, Group};
pub use daily::{DayLog, Habit, HabitId, JournalEntry, MonthJournal};
pub use filter::{Filter, Narrow, Sort};
pub use parse::{
    parse, parse_at, parse_excluding, recurrence_from_phrase, Field, FieldMatch, Guess, ParseResult,
};
pub use recur::{complete_item, complete_occurrence, next_occurrence, occurrences};
pub use settings::{Setting, SettingKind, Settings, Theme};
pub use store::{Snapshot, Store, StoreError, UndoKind, Undone};
pub use task::{
    Checklist, Exception, ExceptionAction, Priority, Recurrence, Subtask, Task, TaskId, WeekdaySet,
};
