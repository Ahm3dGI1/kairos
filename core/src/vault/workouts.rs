//! The workout book as Markdown — one file per routine.
//!
//! ```text
//! # Push
//!
//! ## Exercises
//!
//! - Bench press
//! - Overhead press
//!
//! ## 2026-09-19
//!
//! - Bench press: 10x60, 8x65, 6x70
//! - Overhead press: 12x30
//! > Felt strong.
//! ```
//!
//! Nothing here carries a visible id. A routine is identified by its file, an
//! exercise by its name within that routine, and a session by its date — which
//! is exactly how the spreadsheet this replaces identified them, and it means
//! the file reads like something a person wrote down between sets.

use chrono::NaiveDate;
use uuid::Uuid;

use crate::workout::{
    format_weight, parse_weight, Exercise, ExerciseId, Routine, RoutineId, Session, SessionLog,
    SetEntry,
};

const DATE: &str = "%Y-%m-%d";

/// Fixed namespaces, so a name always derives the same id on every machine.
const ROUTINE_NS: Uuid = Uuid::from_bytes([
    0x6d, 0x74, 0x6f, 0x64, 0x6f, 0x2d, 0x72, 0x6f, 0x75, 0x74, 0x69, 0x6e, 0x65, 0x2d, 0x76, 0x31,
]);

pub fn routine_id(name: &str) -> RoutineId {
    Uuid::new_v5(&ROUTINE_NS, name.trim().to_lowercase().as_bytes())
}

pub fn exercise_id(routine: RoutineId, name: &str) -> ExerciseId {
    Uuid::new_v5(&routine, format!("exercise:{}", name.trim().to_lowercase()).as_bytes())
}

pub fn session_id(routine: RoutineId, date: NaiveDate) -> Uuid {
    Uuid::new_v5(&routine, format!("session:{}", date.format(DATE)).as_bytes())
}

/// Renders one routine's whole history, newest session first.
pub fn write_routine(routine: &Routine, exercises: &[Exercise], sessions: &[SessionLog]) -> String {
    let mut out = format!("# {}\n", routine.name.trim());

    if !exercises.is_empty() {
        out.push_str("\n## Exercises\n\n");
        for exercise in exercises {
            out.push_str(&format!("- {}\n", one_line(&exercise.name)));
        }
    }

    let mut sessions: Vec<&SessionLog> = sessions.iter().collect();
    sessions.sort_by_key(|log| std::cmp::Reverse(log.session.date));

    for log in sessions {
        out.push_str(&format!("\n## {}\n\n", log.session.date.format(DATE)));
        for exercise in exercises {
            let sets = log.for_exercise(exercise.id);
            if sets.is_empty() {
                continue;
            }
            let written: Vec<String> = sets
                .iter()
                .map(|set| format!("{}x{}", set.reps, format_weight(set.weight)))
                .collect();
            out.push_str(&format!("- {}: {}\n", one_line(&exercise.name), written.join(", ")));
        }
        for line in log.session.note.lines() {
            out.push_str(if line.is_empty() { ">" } else { "> " });
            out.push_str(line);
            out.push('\n');
        }
    }

    out
}

/// Everything one routine file describes.
#[derive(Debug, Clone, PartialEq)]
pub struct RoutineFile {
    pub routine: Routine,
    pub exercises: Vec<Exercise>,
    pub sessions: Vec<SessionLog>,
}

/// Reads a routine file. `fallback_name` is used when the file has no heading.
pub fn read_routine(text: &str, fallback_name: &str) -> RoutineFile {
    let mut name = fallback_name.trim().to_string();
    for line in text.lines() {
        if let Some(heading) = line.trim().strip_prefix("# ") {
            if !heading.trim().is_empty() {
                name = heading.trim().to_string();
            }
            break;
        }
    }

    let id = routine_id(&name);
    let routine = Routine { id, name: name.clone(), position: 0 };

    let mut exercises: Vec<Exercise> = Vec::new();
    let mut sessions: Vec<SessionLog> = Vec::new();
    let mut in_exercises = false;
    // Which session the lines being read belong to.
    let mut current: Option<usize> = None;

    for line in text.lines() {
        let trimmed = line.trim();

        if let Some(heading) = trimmed.strip_prefix("## ") {
            let heading = heading.trim();
            in_exercises = false;
            if heading.eq_ignore_ascii_case("exercises") {
                in_exercises = true;
            } else if let Ok(date) = NaiveDate::parse_from_str(heading, DATE) {
                // A session is identified by its date, so a file that names
                // the same date twice is describing one session in two
                // pieces — not two sessions that happen to collide. Carry on
                // filling the one already open.
                current = match sessions.iter().position(|log| log.session.date == date) {
                    Some(index) => Some(index),
                    None => {
                        sessions.push(SessionLog {
                            session: Session {
                                id: session_id(id, date),
                                routine: id,
                                date,
                                note: String::new(),
                            },
                            sets: Vec::new(),
                        });
                        Some(sessions.len() - 1)
                    }
                };
            } else {
                current = None;
            }
            continue;
        }

        if let Some(note) = trimmed.strip_prefix('>') {
            if let Some(log) = current.and_then(|index| sessions.get_mut(index)) {
                if !log.session.note.is_empty() {
                    log.session.note.push('\n');
                }
                log.session.note.push_str(note.strip_prefix(' ').unwrap_or(note));
            }
            continue;
        }

        let Some(rest) = trimmed.strip_prefix("- ") else { continue };

        if in_exercises {
            let name = one_line(rest);
            if !name.is_empty() {
                add_exercise(&mut exercises, id, &name);
            }
            continue;
        }

        // Inside a session: "Bench press: 10x60, 8x65"
        let Some(log) = current.and_then(|index| sessions.get_mut(index)) else { continue };
        let Some((exercise_name, sets)) = rest.split_once(':') else { continue };
        let exercise_name = one_line(exercise_name);
        if exercise_name.is_empty() {
            continue;
        }
        // A session may name an exercise the Exercises list forgot; the list is
        // a convenience, not a gate.
        let exercise = add_exercise(&mut exercises, id, &exercise_name);

        // Numbered from whatever this exercise has already used, so merging
        // two blocks for one date cannot produce two set number ones.
        let mut number = log.sets.iter().filter(|s| s.exercise == exercise).count() as u32;
        for chunk in sets.split(',') {
            let chunk = chunk.trim();
            if chunk.is_empty() {
                continue;
            }
            let Some((reps, weight)) = split_set(chunk) else { continue };
            number += 1;
            log.sets.push(SetEntry { exercise, index: number, reps, weight });
        }
    }

    RoutineFile { routine, exercises, sessions }
}

/// Adds an exercise if this routine has not already seen the name, returning
/// its id either way.
fn add_exercise(into: &mut Vec<Exercise>, routine: RoutineId, name: &str) -> ExerciseId {
    let id = exercise_id(routine, name);
    if !into.iter().any(|e| e.id == id) {
        into.push(Exercise { id, routine, name: name.to_string(), position: into.len() as i64 });
    }
    id
}

/// "10x60", "10 x 60", "10×60", or a bare "10" for a bodyweight set.
fn split_set(text: &str) -> Option<(u32, f64)> {
    let lowered = text.to_lowercase();
    let parts: Vec<&str> = lowered.split(['x', '×', '@']).collect();
    match parts.as_slice() {
        [reps] => Some((reps.trim().parse().ok()?, 0.0)),
        [reps, weight, ..] => {
            Some((reps.trim().parse().ok()?, parse_weight(weight).unwrap_or(0.0)))
        }
        _ => None,
    }
}

fn one_line(text: &str) -> String {
    text.replace(['\n', '\r', ':'], " ").trim().to_string()
}

/// A filename-safe form of a routine name.
pub fn slug(name: &str) -> String {
    let mut slug = String::new();
    for ch in name.trim().to_lowercase().chars() {
        if ch.is_alphanumeric() {
            slug.push(ch);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "routine".to_string()
    } else {
        slug
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> RoutineFile {
        let routine = Routine { id: routine_id("Push"), name: "Push".into(), position: 0 };
        let bench = Exercise {
            id: exercise_id(routine.id, "Bench press"),
            routine: routine.id,
            name: "Bench press".into(),
            position: 0,
        };
        let ohp = Exercise {
            id: exercise_id(routine.id, "Overhead press"),
            routine: routine.id,
            name: "Overhead press".into(),
            position: 1,
        };
        let date = NaiveDate::from_ymd_opt(2026, 9, 19).unwrap();
        let log = SessionLog {
            session: Session {
                id: session_id(routine.id, date),
                routine: routine.id,
                date,
                note: "Felt strong.".into(),
            },
            sets: vec![
                SetEntry { exercise: bench.id, index: 1, reps: 10, weight: 60.0 },
                SetEntry { exercise: bench.id, index: 2, reps: 8, weight: 65.0 },
                SetEntry { exercise: bench.id, index: 3, reps: 6, weight: 72.5 },
                SetEntry { exercise: ohp.id, index: 1, reps: 12, weight: 30.0 },
            ],
        };
        RoutineFile { routine, exercises: vec![bench, ohp], sessions: vec![log] }
    }

    #[test]
    fn a_routine_round_trips() {
        let original = fixture();
        let text = write_routine(&original.routine, &original.exercises, &original.sessions);
        let back = read_routine(&text, "push");

        assert_eq!(back.routine.id, original.routine.id);
        assert_eq!(back.routine.name, "Push");
        assert_eq!(back.exercises.len(), 2);
        assert_eq!(back.exercises[0].name, "Bench press");
        assert_eq!(back.exercises[1].name, "Overhead press");

        assert_eq!(back.sessions.len(), 1);
        let log = &back.sessions[0];
        assert_eq!(log.session.date, original.sessions[0].session.date);
        assert_eq!(log.session.note, "Felt strong.");
        assert_eq!(log.sets.len(), 4);
        let bench = log.for_exercise(original.exercises[0].id);
        assert_eq!(bench.len(), 3);
        assert_eq!((bench[0].reps, bench[0].weight), (10, 60.0));
        assert_eq!((bench[2].reps, bench[2].weight), (6, 72.5), "fractional plates survive");
    }

    #[test]
    fn writing_twice_gives_the_same_text() {
        let original = fixture();
        let once = write_routine(&original.routine, &original.exercises, &original.sessions);
        let back = read_routine(&once, "push");
        let twice = write_routine(&back.routine, &back.exercises, &back.sessions);
        assert_eq!(once, twice);
    }

    #[test]
    fn set_shapes_a_person_would_actually_write() {
        assert_eq!(split_set("10x60"), Some((10, 60.0)));
        assert_eq!(split_set("10 x 60"), Some((10, 60.0)));
        assert_eq!(split_set("10×60"), Some((10, 60.0)));
        assert_eq!(split_set("10@60kg"), Some((10, 60.0)));
        assert_eq!(split_set("12"), Some((12, 0.0)), "bodyweight");
        assert_eq!(split_set("nonsense"), None);
    }

    #[test]
    fn a_session_may_name_an_exercise_the_list_forgot() {
        let text = "# Push\n\n## Exercises\n\n- Bench press\n\n## 2026-09-19\n\n- Dips: 12\n";
        let back = read_routine(text, "push");
        assert_eq!(back.exercises.len(), 2);
        assert!(back.exercises.iter().any(|e| e.name == "Dips"));
        assert_eq!(back.sessions[0].sets.len(), 1);
    }

    #[test]
    fn sessions_are_written_newest_first() {
        let mut fixture = fixture();
        let older = NaiveDate::from_ymd_opt(2026, 9, 12).unwrap();
        fixture.sessions.push(SessionLog {
            session: Session {
                id: session_id(fixture.routine.id, older),
                routine: fixture.routine.id,
                date: older,
                note: String::new(),
            },
            sets: vec![],
        });
        let text = write_routine(&fixture.routine, &fixture.exercises, &fixture.sessions);
        let newest = text.find("2026-09-19").unwrap();
        let oldest = text.find("2026-09-12").unwrap();
        assert!(newest < oldest, "the most recent session reads first");
    }

    #[test]
    fn ids_are_derived_from_names_not_random() {
        assert_eq!(routine_id("Push"), routine_id("push"));
        assert_ne!(routine_id("Push"), routine_id("Pull"));
        let push = routine_id("Push");
        assert_eq!(exercise_id(push, "Dips"), exercise_id(push, "dips"));
        assert_ne!(exercise_id(push, "Dips"), exercise_id(routine_id("Pull"), "Dips"));
    }

    #[test]
    fn slugs_are_filename_safe() {
        assert_eq!(slug("Push"), "push");
        assert_eq!(slug("Leg / Core day"), "leg-core-day");
        assert_eq!(slug("???"), "routine");
    }
}
