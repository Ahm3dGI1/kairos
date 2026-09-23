mod date;
mod markers;
mod recurrence;
mod time;
mod token;
mod words;

use std::ops::Range;

use chrono::{Local, NaiveDateTime};

use crate::task::Task;
use token::{span_text, tokenize_excluding, Token};

/// A field of [`Task`] that the parser can fill in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Title,
    Date,
    Time,
    Recurrence,
    Tag,
    Project,
    Priority,
}

/// Where a parsed field came from in the input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldMatch {
    pub field: Field,
    /// Byte range in the original input.
    pub span: Range<usize>,
    /// The matched text, as written.
    pub text: String,
}

/// Something the parser decided that the input did not actually say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Guess {
    pub field: Field,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseResult {
    pub task: Task,
    pub input: String,
    /// One entry per field the parser filled, in extraction order.
    pub matches: Vec<FieldMatch>,
    /// Inferences a client may want to surface for confirmation.
    pub guesses: Vec<Guess>,
}

impl ParseResult {
    /// Whether this field rests on an inference rather than on what was written.
    pub fn is_guessed(&self, field: Field) -> bool {
        self.guesses.iter().any(|g| g.field == field)
    }

    pub fn matched(&self, field: Field) -> Option<&FieldMatch> {
        self.matches.iter().find(|m| m.field == field)
    }

    fn push_match(&mut self, field: Field, span: Range<usize>) {
        let text = span_text(&self.input, &span);
        self.matches.push(FieldMatch { field, span, text });
    }
}

/// Parses a line against the current local time.
pub fn parse(input: &str) -> ParseResult {
    parse_at(input, Local::now().naive_local())
}

/// Parses a line against an explicit "now" — the entry point tests use, and the
/// one to prefer anywhere the caller already knows the user's local time.
pub fn parse_at(input: &str, now: NaiveDateTime) -> ParseResult {
    parse_excluding(input, now, &[])
}

/// Parses a line, leaving the given byte ranges as plain text.
pub fn parse_excluding(input: &str, now: NaiveDateTime, excluded: &[Range<usize>]) -> ParseResult {
    let mut tokens = tokenize_excluding(input, excluded);
    let mut result = ParseResult {
        task: Task::new("", now.date()),
        input: input.to_string(),
        matches: Vec::new(),
        guesses: Vec::new(),
    };

    let markers = markers::extract(&mut tokens);
    for (tag, span) in markers.tags {
        result.task.add_tag(tag);
        result.push_match(Field::Tag, span);
    }
    if let Some((project, span)) = markers.project {
        result.task.project = Some(project);
        result.push_match(Field::Project, span);
    }
    if let Some((priority, span)) = markers.priority {
        result.task.priority = priority;
        result.push_match(Field::Priority, span);
    }

    if let Some((rule, span)) = recurrence::extract(&mut tokens) {
        result.task.recurrence = Some(rule);
        result.push_match(Field::Recurrence, span);
    }

    if let Some(found) = time::extract(&mut tokens) {
        result.task.time = Some(found.time);
        if found.meridiem_guessed {
            let written = span_text(input, &found.span);
            result.guesses.push(Guess {
                field: Field::Time,
                note: format!("\"{written}\" read as {}", found.time.format("%H:%M")),
            });
        }
        result.push_match(Field::Time, found.span);
    }

    if let Some(found) = date::extract(&mut tokens, now.date()) {
        result.task.due = Some(found.date);
        for note in found.guesses {
            result.guesses.push(Guess { field: Field::Date, note });
        }
        result.push_match(Field::Date, found.span);
    }

    // A recurring task with no date named starts today: "gym every day" means
    // starting now, not at some unstated point.
    if result.task.recurrence.is_some() && result.task.due.is_none() {
        result.task.due = Some(now.date());
    }

    // A bare time means the next time that clock time comes round. "call mom at
    // 5pm" is today at four o'clock and tomorrow at six — never a dateless task
    // that no view would show and no reminder could fire.
    if let (Some(time), None, None) = (result.task.time, result.task.due, result.task.recurrence) {
        let today = now.date();
        let due = if time > now.time() { Some(today) } else { today.succ_opt() };
        if let Some(due) = due {
            result.task.due = due.into();
            if due != today {
                result.guesses.push(Guess {
                    field: Field::Date,
                    note: format!("{} has passed today; due tomorrow", time.format("%H:%M")),
                });
            }
        }
    }

    result.task.title = title_from(&tokens);
    result
}

/// Reads a bare recurrence phrase — "every day", "every monday" — as a rule.
pub fn recurrence_from_phrase(phrase: &str) -> Option<crate::task::Recurrence> {
    let mut tokens = tokenize_excluding(phrase, &[]);
    recurrence::extract(&mut tokens).map(|(rule, _)| rule)
}

/// Prepositions that only made sense as part of a phrase the extractors took;
/// left dangling at either end of the title, they are noise.
const EDGE_FILLER: &[&str] =
    &["at", "on", "in", "by", "of", "for", "from", "starting", "and", "the"];

fn title_from(tokens: &[Token]) -> String {
    let mut remaining: Vec<&Token> = tokens.iter().filter(|t| !t.consumed).collect();

    while remaining.first().is_some_and(|t| EDGE_FILLER.contains(&t.text.as_str())) {
        remaining.remove(0);
    }
    while remaining.last().is_some_and(|t| EDGE_FILLER.contains(&t.text.as_str())) {
        remaining.pop();
    }

    let joined = remaining.iter().map(|t| t.raw.as_str()).collect::<Vec<_>>().join(" ");
    joined.trim().trim_matches(|c: char| ",;-–—".contains(c)).trim().to_string()
}
