use std::ops::Range;

use chrono::NaiveTime;

use super::token::{consume, Token};

pub struct TimeMatch {
    pub time: NaiveTime,
    pub span: Range<usize>,
    /// True when am/pm was not written and had to be guessed.
    pub meridiem_guessed: bool,
}

pub fn extract(tokens: &mut [Token]) -> Option<TimeMatch> {
    for i in 0..tokens.len() {
        if !tokens[i].available() {
            continue;
        }
        if let Some((time, range, guessed)) = match_at(tokens, i) {
            let span = consume(tokens, range);
            return Some(TimeMatch { time, span, meridiem_guessed: guessed });
        }
    }
    None
}

fn match_at(tokens: &[Token], i: usize) -> Option<(NaiveTime, Range<usize>, bool)> {
    // "at" immediately before the number is both a marker and part of the match.
    let has_at = i > 0 && tokens[i - 1].is("at");
    let start = if has_at { i - 1 } else { i };

    if tokens[i].is_any(&["noon", "midday"]) {
        return Some((NaiveTime::from_hms_opt(12, 0, 0)?, start..i + 1, false));
    }
    if tokens[i].is("midnight") {
        return Some((NaiveTime::from_hms_opt(0, 0, 0)?, start..i + 1, false));
    }

    let clock = parse_clock(&tokens[i].text)?;
    let mut end = i + 1;
    let mut pm = clock.pm;

    // A meridiem written as its own token: "5 pm".
    if pm.is_none() {
        if let Some(next) = tokens.get(end) {
            if let Some(p) = meridiem(&next.text) {
                pm = Some(p);
                end += 1;
            }
        }
    }

    let (hour, guessed) = match pm {
        Some(true) => (if clock.hour == 12 { 12 } else { clock.hour + 12 }, false),
        Some(false) => (if clock.hour == 12 { 0 } else { clock.hour }, false),
        // No meridiem: a colon reads as 24-hour, "at" licenses a guess, and an
        // unmarked bare number is left alone.
        None if clock.had_colon => (clock.hour, false),
        None if has_at => (guess_hour(clock.hour), clock.hour < 13),
        None => return None,
    };

    let time = NaiveTime::from_hms_opt(hour, clock.minute, 0)?;
    Some((time, start..end, guessed))
}

struct Clock {
    hour: u32,
    minute: u32,
    pm: Option<bool>,
    had_colon: bool,
}

/// Parses "5", "5:30", "5pm", "5:30pm", "17:00". Rejects anything else.
fn parse_clock(text: &str) -> Option<Clock> {
    let compact = text.replace('.', "");
    let (body, pm) = match compact.strip_suffix("pm") {
        Some(body) => (body, Some(true)),
        None => match compact.strip_suffix("am") {
            Some(body) => (body, Some(false)),
            None => (compact.as_str(), None),
        },
    };

    let (hour_str, minute_str) = match body.split_once(':') {
        Some((h, m)) => (h, Some(m)),
        None => (body, None),
    };
    let had_colon = minute_str.is_some();

    let hour: u32 = hour_str.parse().ok()?;
    let minute: u32 = match minute_str {
        Some(m) if m.len() == 2 => m.parse().ok()?,
        Some(_) => return None,
        None => 0,
    };
    // A written meridiem constrains the hour to a 12-hour clock.
    let hour_limit = if pm.is_some() { 12 } else { 23 };
    if hour > hour_limit || minute > 59 {
        return None;
    }
    Some(Clock { hour, minute, pm, had_colon })
}

fn meridiem(text: &str) -> Option<bool> {
    match text.replace('.', "").as_str() {
        "pm" => Some(true),
        "am" => Some(false),
        _ => None,
    }
}

/// The guess for an unmarked hour: 1-6 reads as afternoon, 7-11 as morning.
/// 13 and up is already unambiguous.
fn guess_hour(hour: u32) -> u32 {
    match hour {
        1..=6 => hour + 12,
        _ => hour,
    }
}
