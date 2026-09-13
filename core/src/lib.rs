//! Shared business logic for Master Todo App.
//!
//! Everything a client needs to know about what a task *is* lives here: the
//! model, and the natural-language parsing that turns a single typed line into
//! one. No UI, no storage, no platform APIs — this crate compiles the same for
//! the Windows shell, a Linux TUI, and (later, over FFI) mobile.
//!
//! ```
//! use chrono::{NaiveDate, NaiveTime};
//! use mtodo_core::{parse_at, Recurrence};
//!
//! let now = NaiveDate::from_ymd_opt(2026, 9, 13).unwrap().and_hms_opt(9, 0, 0).unwrap();
//! let result = parse_at("gym every day 5pm", now);
//!
//! assert_eq!(result.task.title, "gym");
//! assert_eq!(result.task.recurrence, Some(Recurrence::Daily));
//! assert_eq!(result.task.time, NaiveTime::from_hms_opt(17, 0, 0));
//! ```
//!
//! Times and dates are naive local values: the core does not carry a timezone,
//! and the shell supplies one. That holds until multi-device sync forces the
//! question — see `docs/todo-app-spec.md` §6.

pub mod parse;
pub mod task;

pub use parse::{parse, parse_at, Field, FieldMatch, Guess, ParseResult};
pub use task::{Recurrence, Task};
