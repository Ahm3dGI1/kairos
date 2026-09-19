//! Strength training, modelled the way the spreadsheet it replaces was.
//!
//! A **routine** is a page — push day, pull day, leg day. An **exercise** is a
//! row on that page, and the rows stay the same week to week, which is the
//! whole point of a routine. A **session** is one dated instance of the page,
//! and within it each exercise has a handful of **sets**: a rep count and a
//! weight.
//!
//! This is deliberately not modelled as tasks. A task is a thing you finish; a
//! set is a measurement, and the useful question about it is not "is it done"
//! but "was it more than last time". Forcing it into the task model would lose
//! the numbers, which are the only reason to record any of it.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type RoutineId = Uuid;
pub type ExerciseId = Uuid;
pub type SessionId = Uuid;

/// A page of the workout book: the exercises you do together on a given day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Routine {
    pub id: RoutineId,
    pub name: String,
    pub position: i64,
}

impl Routine {
    pub fn new(name: impl Into<String>, position: i64) -> Self {
        Self { id: Uuid::new_v4(), name: name.into(), position }
    }
}

/// One movement, and the row it occupies on its routine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exercise {
    pub id: ExerciseId,
    pub routine: RoutineId,
    pub name: String,
    pub position: i64,
}

impl Exercise {
    pub fn new(routine: RoutineId, name: impl Into<String>, position: i64) -> Self {
        Self { id: Uuid::new_v4(), routine, name: name.into(), position }
    }
}

/// One time through a routine, on a date.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub routine: RoutineId,
    pub date: NaiveDate,
    pub note: String,
}

impl Session {
    pub fn new(routine: RoutineId, date: NaiveDate) -> Self {
        Self { id: Uuid::new_v4(), routine, date, note: String::new() }
    }
}

/// One set: which exercise, which number in the run, and what you actually did.
///
/// Weight is unitless on purpose — it is whatever the user's gym uses, and
/// converting it would only ever be a way to get it wrong.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SetEntry {
    pub exercise: ExerciseId,
    /// 1-based, as it is counted out loud.
    pub index: u32,
    pub reps: u32,
    pub weight: f64,
}

impl SetEntry {
    /// Reps times weight — the one number that compares two sets honestly
    /// enough to be worth showing.
    pub fn volume(&self) -> f64 {
        f64::from(self.reps) * self.weight
    }
}

/// Everything one session did, grouped by exercise.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionLog {
    pub session: Session,
    pub sets: Vec<SetEntry>,
}

impl SessionLog {
    pub fn for_exercise(&self, exercise: ExerciseId) -> Vec<SetEntry> {
        let mut sets: Vec<SetEntry> =
            self.sets.iter().copied().filter(|s| s.exercise == exercise).collect();
        sets.sort_by_key(|s| s.index);
        sets
    }

    pub fn volume(&self) -> f64 {
        self.sets.iter().map(SetEntry::volume).sum()
    }

    /// The heaviest weight moved for an exercise, which is what people
    /// actually remember about a session.
    pub fn top_weight(&self, exercise: ExerciseId) -> Option<f64> {
        self.sets
            .iter()
            .filter(|s| s.exercise == exercise)
            .map(|s| s.weight)
            .fold(None, |best: Option<f64>, w| Some(best.map_or(w, |b| b.max(w))))
    }
}

/// Formats a weight the way it would be written down: no trailing zero unless
/// the plates actually were fractional.
pub fn format_weight(weight: f64) -> String {
    if (weight - weight.round()).abs() < f64::EPSILON {
        format!("{}", weight.round() as i64)
    } else {
        format!("{weight:.1}")
    }
}

/// Reads "60", "60kg", "62.5" — the shapes someone types between sets.
pub fn parse_weight(text: &str) -> Option<f64> {
    let cleaned: String = text
        .trim()
        .to_lowercase()
        .trim_end_matches("kg")
        .trim_end_matches("lb")
        .trim_end_matches("lbs")
        .replace(',', ".")
        .trim()
        .to_string();
    if cleaned.is_empty() {
        return None;
    }
    cleaned.parse::<f64>().ok().filter(|w| *w >= 0.0)
}
