//! Single-line natural-language parsing: "gym every day 5pm" in, a structured
//! [`Task`] out.
//!
//! The pipeline runs recurrence, then time, then date, each claiming the tokens
//! it recognizes. Whatever is left over is the title — which is why order
//! matters: "every monday" has to become a recurrence before "monday" can be
//! read as a due date.
//!
//! Ambiguity is resolved by guessing rather than by asking, because capture
//! speed is the point of the app. Every guess is recorded in
//! [`ParseResult::guesses`] so a client can show what it inferred, and
//! [`ParseResult::matches`] says which slice of the input produced each field
//! so any of it can be overridden.

mod date;
mod recurrence;
mod time;
mod token;
mod words;

use std::ops::Range;

use chrono::{Local, NaiveDateTime};

use crate::task::Task;
use token::{span_text, tokenize, Token};

/// A field of [`Task`] that the parser can fill in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Title,
    Date,
    Time,
    Recurrence,
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
}

/// Parses a line against the current local time.
pub fn parse(input: &str) -> ParseResult {
    parse_at(input, Local::now().naive_local())
}

/// Parses a line against an explicit "now" — the entry point tests use, and the
/// one to prefer anywhere the caller already knows the user's local time.
pub fn parse_at(input: &str, now: NaiveDateTime) -> ParseResult {
    let mut tokens = tokenize(input);
    let mut result = ParseResult {
        task: Task::default(),
        input: input.to_string(),
        matches: Vec::new(),
        guesses: Vec::new(),
    };

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
        result.task.date = Some(found.date);
        for note in found.guesses {
            result.guesses.push(Guess { field: Field::Date, note });
        }
        result.push_match(Field::Date, found.span);
    }

    result.task.title = title_from(&tokens);
    result
}

impl ParseResult {
    fn push_match(&mut self, field: Field, span: Range<usize>) {
        let text = span_text(&self.input, &span);
        self.matches.push(FieldMatch { field, span, text });
    }
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
