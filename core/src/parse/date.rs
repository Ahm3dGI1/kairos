use std::ops::Range;

use chrono::{Datelike, NaiveDate, Weekday};

use super::token::{consume, word_at, Token};
use super::words;

pub struct DateMatch {
    pub date: NaiveDate,
    pub span: Range<usize>,
    /// Guesses made to reach this date, in the order they were made.
    pub guesses: Vec<String>,
}

pub fn extract(tokens: &mut [Token], today: NaiveDate) -> Option<DateMatch> {
    for i in 0..tokens.len() {
        if !tokens[i].available() {
            continue;
        }
        if let Some((date, len, guesses)) = match_at(tokens, i, today) {
            // "on friday" — fold the preposition into the match so it does not
            // survive into the title.
            let start = if i > 0 && tokens[i - 1].is_any(&["on", "by"]) { i - 1 } else { i };
            let span = consume(tokens, start..i + len);
            return Some(DateMatch { date, span, guesses });
        }
    }
    None
}

fn match_at(
    tokens: &[Token],
    i: usize,
    today: NaiveDate,
) -> Option<(NaiveDate, usize, Vec<String>)> {
    let word = tokens[i].text.as_str();
    let plain = |date: NaiveDate, len: usize| Some((date, len, Vec::new()));

    match word {
        "today" => return plain(today, 1),
        "tonight" => {
            let note = "\"tonight\" set the date only; no time was inferred".to_string();
            return Some((today, 1, vec![note]));
        }
        "tomorrow" | "tmrw" | "tmr" => return plain(today.succ_opt()?, 1),
        "yesterday" => return plain(today.pred_opt()?, 1),
        _ => {}
    }

    // ISO dates need no interpretation at all.
    if let Ok(date) = NaiveDate::parse_from_str(word, "%Y-%m-%d") {
        return plain(date, 1);
    }

    // "next friday" / "this friday" / "next week"
    if matches!(word, "next" | "this" | "coming") {
        let qualifier = word;
        let next = word_at(tokens, i + 1)?;
        if let Some(weekday) = words::weekday(next) {
            // "next friday" always skips a friday that is today; "this friday"
            // and "coming friday" accept it.
            let include_today = qualifier != "next";
            let date = next_weekday(today, weekday, include_today);
            let note = format!("\"{qualifier} {next}\" resolved to {date}");
            return Some((date, 2, vec![note]));
        }
        let date = match next {
            "week" => today.checked_add_signed(chrono::Duration::days(7))?,
            "month" => add_months(today, 1),
            "year" => add_months(today, 12),
            _ => return None,
        };
        if qualifier == "next" {
            return plain(date, 2);
        }
        return None;
    }

    // "in 3 days" / "in two weeks"
    if word == "in" {
        let n = words::cardinal(word_at(tokens, i + 1)?)?;
        let unit = word_at(tokens, i + 2)?;
        let date = match unit {
            "day" | "days" => today.checked_add_signed(chrono::Duration::days(n as i64))?,
            "week" | "weeks" => today.checked_add_signed(chrono::Duration::try_weeks(n as i64)?)?,
            "month" | "months" => add_months(today, i32::try_from(n).ok()?),
            "year" | "years" => add_months(today, i32::try_from(n).ok()?.checked_mul(12)?),
            _ => return None,
        };
        return plain(date, 3);
    }

    // A bare weekday: the next one, counting today.
    if let Some(weekday) = words::weekday(word) {
        let date = next_weekday(today, weekday, true);
        let note = format!("\"{word}\" resolved to {date}");
        return Some((date, 1, vec![note]));
    }

    // "march 5", "march 5th 2027"
    if let Some(month) = words::month(word) {
        let day = words::day_number(word_at(tokens, i + 1)?)?;
        let (year, len, guesses) = year_after(tokens, i + 2, today, month, day);
        return Some((NaiveDate::from_ymd_opt(year, month, day)?, 2 + len, guesses));
    }

    // "5 march", "5th of march"
    if let Some(day) = words::day_number(word) {
        let mut j = i + 1;
        if tokens.get(j).is_some_and(|t| t.is("of")) {
            j += 1;
        }
        let month = words::month(word_at(tokens, j)?)?;
        let (year, len, guesses) = year_after(tokens, j + 1, today, month, day);
        let span = j + 1 - i + len;
        return Some((NaiveDate::from_ymd_opt(year, month, day)?, span, guesses));
    }

    None
}

/// Reads an explicit year at `i` if there is one; otherwise picks the year that
/// puts the date in the future, and says so.
fn year_after(
    tokens: &[Token],
    i: usize,
    today: NaiveDate,
    month: u32,
    day: u32,
) -> (i32, usize, Vec<String>) {
    if let Some(year) = word_at(tokens, i).and_then(|w| w.parse::<i32>().ok()) {
        if (1000..=9999).contains(&year) {
            return (year, 1, Vec::new());
        }
    }
    let this_year = NaiveDate::from_ymd_opt(today.year(), month, day);
    match this_year {
        Some(date) if date >= today => (today.year(), 0, Vec::new()),
        _ => {
            let year = today.year() + 1;
            let note = format!("no year given; assumed {year}");
            (year, 0, vec![note])
        }
    }
}

fn next_weekday(from: NaiveDate, weekday: Weekday, include_today: bool) -> NaiveDate {
    let mut date = if include_today { from } else { from.succ_opt().unwrap_or(from) };
    while date.weekday() != weekday {
        date = match date.succ_opt() {
            Some(next) => next,
            None => return date,
        };
    }
    date
}

/// Month arithmetic that clamps rather than overflowing: Jan 31 + 1 month is
/// the last day of February.
fn add_months(date: NaiveDate, months: i32) -> NaiveDate {
    let Some(total) = (date.year() * 12 + date.month0() as i32).checked_add(months) else {
        return date;
    };
    let year = total.div_euclid(12);
    let month = total.rem_euclid(12) as u32 + 1;
    let day = date.day().min(days_in_month(year, month));
    NaiveDate::from_ymd_opt(year, month, day).unwrap_or(date)
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let (next_year, next_month) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
    NaiveDate::from_ymd_opt(next_year, next_month, 1)
        .and_then(|first| first.pred_opt())
        .map_or(28, |last| last.day())
}
