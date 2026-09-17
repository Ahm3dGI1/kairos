//! Sigil markers: `#tag`, `@project`, `!priority`.
//!
//! These run before the date and time extractors. They are unambiguous — a
//! sigil means exactly one thing — so claiming them first keeps a tag like
//! `#friday` or a project like `@march` from being read as a date.

use std::ops::Range;

use super::token::{consume, Token};
use crate::task::Priority;

#[derive(Debug, Default)]
pub struct Markers {
    pub tags: Vec<(String, Range<usize>)>,
    pub project: Option<(String, Range<usize>)>,
    pub priority: Option<(Priority, Range<usize>)>,
}

pub fn extract(tokens: &mut [Token]) -> Markers {
    let mut found = Markers::default();

    for i in 0..tokens.len() {
        if tokens[i].consumed {
            continue;
        }
        let text = tokens[i].text.clone();

        // A bare sigil is punctuation the user typed on its own; ignore it.
        let Some((sigil, rest)) = split_sigil(&text) else { continue };
        if rest.is_empty() && sigil != '!' {
            continue;
        }

        match sigil {
            '#' => {
                let span = consume(tokens, i..i + 1);
                found.tags.push((rest.to_string(), span));
            }
            '@' => {
                // The first project wins; a second @ is left in the title
                // rather than silently overwriting the user's first choice.
                if found.project.is_none() {
                    let span = consume(tokens, i..i + 1);
                    found.project = Some((rest.to_string(), span));
                }
            }
            '!' => {
                // "!" through "!!!" are themselves the priority, so a run of
                // them is read whole; "!p1" and "!high" name it instead.
                // Anything else is ordinary punctuation.
                let word = if rest.chars().all(|c| c == '!') { text.as_str() } else { rest };
                if let Some(priority) = Priority::parse(word) {
                    if found.priority.is_none() {
                        let span = consume(tokens, i..i + 1);
                        found.priority = Some((priority, span));
                    }
                }
            }
            _ => {}
        }
    }
    found
}

/// Splits a leading sigil off a token, if it carries one.
fn split_sigil(text: &str) -> Option<(char, &str)> {
    let mut chars = text.chars();
    let sigil = chars.next()?;
    matches!(sigil, '#' | '@' | '!').then(|| (sigil, chars.as_str()))
}
