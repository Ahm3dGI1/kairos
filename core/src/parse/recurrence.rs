//! Recurrence phrases. Extracted first, because "every monday" must claim the
//! weekday before the date extractor reads it as a due date.

use std::ops::Range;

use super::token::{consume, word_at, Token};
use super::words;
use crate::task::Recurrence;

pub fn extract(tokens: &mut [Token]) -> Option<(Recurrence, Range<usize>)> {
    for i in 0..tokens.len() {
        if tokens[i].consumed {
            continue;
        }
        if let Some((rec, len)) = match_at(tokens, i) {
            let span = consume(tokens, i..i + len);
            return Some((rec.normalized(), span));
        }
    }
    None
}

/// Tries to match a recurrence phrase starting at `i`, returning the rule and
/// how many tokens it spans.
fn match_at(tokens: &[Token], i: usize) -> Option<(Recurrence, usize)> {
    let word = tokens[i].text.as_str();

    // Single-word forms.
    match word {
        "daily" | "everyday" => return Some((Recurrence::Daily, 1)),
        "weekly" => return Some((Recurrence::Weekly { weekday: None }, 1)),
        "biweekly" | "fortnightly" => {
            return Some((Recurrence::EveryNWeeks { n: 2, weekday: None }, 1))
        }
        "monthly" => return Some((Recurrence::Monthly { day: None }, 1)),
        "yearly" | "annually" => return Some((Recurrence::Yearly, 1)),
        _ => {}
    }
    // "mondays" reads as a recurrence on its own; bare "monday" is a due date.
    if let Some(weekday) = words::plural_weekday(word) {
        return Some((Recurrence::Weekly { weekday: Some(weekday) }, 1));
    }

    if word != "every" {
        return None;
    }

    // "every single day" — skip the intensifier and carry on.
    let mut j = i + 1;
    if tokens.get(j).is_some_and(|t| t.is("single")) {
        j += 1;
    }
    let next = word_at(tokens, j)?;

    // "every 15th" — a day of the month, not a count.
    if let Some(day) = words::ordinal_day(next) {
        return Some((Recurrence::Monthly { day: Some(day) }, j + 1 - i));
    }

    // "every 3 days", "every other week", "every other tuesday"
    let (rec, end) = if let Some(n) = words::cardinal(next) {
        let unit = word_at(tokens, j + 1)?;
        let rec = match unit {
            "day" | "days" => Recurrence::EveryNDays(n),
            "week" | "weeks" => Recurrence::EveryNWeeks { n, weekday: None },
            "month" | "months" => Recurrence::EveryNMonths(n),
            "year" | "years" if n == 1 => Recurrence::Yearly,
            _ => match words::weekday(unit) {
                Some(weekday) => Recurrence::EveryNWeeks { n, weekday: Some(weekday) },
                None => return None,
            },
        };
        (rec, j + 2)
    } else {
        let rec = match next {
            "day" => Recurrence::Daily,
            "week" => Recurrence::Weekly { weekday: None },
            "month" => Recurrence::Monthly { day: None },
            "year" => Recurrence::Yearly,
            "weekday" | "weekdays" => Recurrence::Weekdays,
            "weekend" | "weekends" => Recurrence::Weekends,
            _ => Recurrence::Weekly { weekday: Some(words::weekday(next)?) },
        };
        (rec, j + 1)
    };

    let (rec, end) = anchor(tokens, end, rec).unwrap_or((rec, end));
    Some((rec, end - i))
}

/// Pins an otherwise unanchored rule to the day it names: "every month on the
/// 1st", "every 2 weeks on monday". Returns the rule and the index just past it.
fn anchor(tokens: &[Token], mut j: usize, rec: Recurrence) -> Option<(Recurrence, usize)> {
    if word_at(tokens, j) == Some("on") {
        j += 1;
    }
    if word_at(tokens, j) == Some("the") {
        j += 1;
    }
    let word = word_at(tokens, j)?;

    let anchored = match rec {
        Recurrence::Monthly { day: None } => {
            Recurrence::Monthly { day: Some(words::ordinal_day(word)?) }
        }
        Recurrence::Weekly { weekday: None } => {
            Recurrence::Weekly { weekday: Some(words::weekday(word)?) }
        }
        Recurrence::EveryNWeeks { n, weekday: None } => {
            Recurrence::EveryNWeeks { n, weekday: Some(words::weekday(word)?) }
        }
        _ => return None,
    };
    Some((anchored, j + 1))
}
