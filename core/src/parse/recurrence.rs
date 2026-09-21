//! Recurrence phrases. Extracted before dates, because "every monday" must
//! claim the weekday before the date extractor reads it as a due date.

use std::ops::Range;

use super::token::{consume, word_at, Token};
use super::words;
use crate::task::{Recurrence, WeekdaySet};

pub fn extract(tokens: &mut [Token]) -> Option<(Recurrence, Range<usize>)> {
    for i in 0..tokens.len() {
        if !tokens[i].available() {
            continue;
        }
        if let Some((rule, end)) = match_at(tokens, i) {
            let span = consume(tokens, i..end);
            return Some((rule.normalized(), span));
        }
    }
    None
}

/// Tries to match a recurrence phrase starting at `i`, returning the rule and
/// the index just past it.
fn match_at(tokens: &[Token], i: usize) -> Option<(Recurrence, usize)> {
    let word = word_at(tokens, i)?;

    // Single-word forms.
    let simple = match word {
        "daily" | "everyday" => Some(Recurrence::Daily),
        "weekly" => Some(Recurrence::Weekly { days: WeekdaySet::EMPTY }),
        "biweekly" | "fortnightly" => {
            Some(Recurrence::EveryNWeeks { n: 2, days: WeekdaySet::EMPTY })
        }
        "monthly" => Some(Recurrence::Monthly { day: None }),
        "yearly" | "annually" => Some(Recurrence::Yearly),
        _ => None,
    };
    if let Some(rule) = simple {
        return Some(finish(tokens, rule, i + 1));
    }

    // "mondays" reads as a recurrence on its own; bare "monday" is a due date.
    if let Some(day) = words::plural_weekday(word) {
        let mut days = WeekdaySet::from_day(day);
        let end = extend_weekdays(tokens, i + 1, &mut days);
        return Some(finish(tokens, Recurrence::Weekly { days }, end));
    }

    if word != "every" {
        return None;
    }

    // "every single day" — skip the intensifier and carry on.
    let mut j = i + 1;
    if word_at(tokens, j) == Some("single") {
        j += 1;
    }
    let next = word_at(tokens, j)?;

    // "every 15th" — a day of the month, not a count.
    if let Some(day) = words::ordinal_day(next) {
        return Some((Recurrence::Monthly { day: Some(day) }, j + 1));
    }

    // "every 3 days", "every other week", "every other tuesday"
    if let Some(n) = words::cardinal(next) {
        let unit = word_at(tokens, j + 1)?;
        let rule = match unit {
            "day" | "days" => Recurrence::EveryNDays(n),
            "week" | "weeks" => Recurrence::EveryNWeeks { n, days: WeekdaySet::EMPTY },
            "month" | "months" => Recurrence::EveryNMonths(n),
            "year" | "years" if n == 1 => Recurrence::Yearly,
            _ => {
                let mut days = words::weekday_run(unit)?;
                let end = extend_weekdays(tokens, j + 2, &mut days);
                return Some(finish(tokens, Recurrence::EveryNWeeks { n, days }, end));
            }
        };
        return Some(finish(tokens, rule, j + 2));
    }

    let rule = match next {
        "day" => Recurrence::Daily,
        "week" => Recurrence::Weekly { days: WeekdaySet::EMPTY },
        "month" => Recurrence::Monthly { day: None },
        "year" => Recurrence::Yearly,
        "weekday" | "weekdays" => Recurrence::WEEKDAYS,
        "weekend" | "weekends" => Recurrence::WEEKENDS,
        _ => {
            let mut days = words::weekday_run(next)?;
            let end = extend_weekdays(tokens, j + 1, &mut days);
            return Some(finish(tokens, Recurrence::Weekly { days }, end));
        }
    };
    Some(finish(tokens, rule, j + 1))
}

/// Absorbs further weekdays into a set: "every monday and wednesday", and the
/// comma form "every monday, wednesday and friday" — the tokenizer has already
/// stripped the commas, so a bare run of weekdays reads the same way.
fn extend_weekdays(tokens: &[Token], mut j: usize, days: &mut WeekdaySet) -> usize {
    loop {
        let mut k = j;
        if word_at(tokens, k).is_some_and(|w| matches!(w, "and" | "&" | "plus")) {
            k += 1;
        }
        match word_at(tokens, k).and_then(words::weekday_run) {
            Some(more) => {
                *days = days.union(more);
                j = k + 1;
            }
            None => return j,
        }
    }
}

/// Applies the trailing anchor phrase if there is one.
fn finish(tokens: &[Token], rule: Recurrence, end: usize) -> (Recurrence, usize) {
    anchor(tokens, end, rule).unwrap_or((rule, end))
}

/// Pins an otherwise unanchored rule to the day it names: "every month on the
/// 1st", "every 2 weeks on monday". Returns the rule and the index just past it.
fn anchor(tokens: &[Token], mut j: usize, rule: Recurrence) -> Option<(Recurrence, usize)> {
    if word_at(tokens, j) == Some("on") {
        j += 1;
    }
    if word_at(tokens, j) == Some("the") {
        j += 1;
    }
    let word = word_at(tokens, j)?;

    let anchored = match rule {
        Recurrence::Monthly { day: None } => {
            Recurrence::Monthly { day: Some(words::ordinal_day(word)?) }
        }
        Recurrence::Weekly { days } if days.is_empty() => {
            let mut days = words::weekday_run(word)?;
            let end = extend_weekdays(tokens, j + 1, &mut days);
            return Some((Recurrence::Weekly { days }, end));
        }
        Recurrence::EveryNWeeks { n, days } if days.is_empty() => {
            let mut days = words::weekday_run(word)?;
            let end = extend_weekdays(tokens, j + 1, &mut days);
            return Some((Recurrence::EveryNWeeks { n, days }, end));
        }
        _ => return None,
    };
    Some((anchored, j + 1))
}
