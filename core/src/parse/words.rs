//! Word-level vocabulary: weekdays, months, and the number forms that show up
//! in date and recurrence phrases.

use chrono::Weekday;

/// Recognizes a weekday name or common abbreviation. Accepts a trailing "s"
/// ("mondays") because the recurrence extractor needs the plural form too.
pub fn weekday(word: &str) -> Option<Weekday> {
    let w = word.strip_suffix('s').unwrap_or(word);
    Some(match w {
        "monday" | "mon" => Weekday::Mon,
        "tuesday" | "tue" | "tues" => Weekday::Tue,
        "wednesday" | "wed" | "weds" => Weekday::Wed,
        "thursday" | "thu" | "thur" | "thurs" => Weekday::Thu,
        "friday" | "fri" => Weekday::Fri,
        "saturday" | "sat" => Weekday::Sat,
        "sunday" | "sun" => Weekday::Sun,
        _ => return None,
    })
}

/// Recognizes only the plural form ("mondays"), which reads as a recurrence on
/// its own where the singular does not.
///
/// Full names only: "tues" and "weds" are singular abbreviations that happen to
/// end in an "s", and reading them as recurrences would be wrong.
pub fn plural_weekday(word: &str) -> Option<Weekday> {
    let stem = word.strip_suffix('s')?;
    let full = matches!(
        stem,
        "monday" | "tuesday" | "wednesday" | "thursday" | "friday" | "saturday" | "sunday"
    );
    full.then(|| weekday(stem)).flatten()
}

pub fn month(word: &str) -> Option<u32> {
    Some(match word {
        "january" | "jan" => 1,
        "february" | "feb" => 2,
        "march" | "mar" => 3,
        "april" | "apr" => 4,
        "may" => 5,
        "june" | "jun" => 6,
        "july" | "jul" => 7,
        "august" | "aug" => 8,
        "september" | "sep" | "sept" => 9,
        "october" | "oct" => 10,
        "november" | "nov" => 11,
        "december" | "dec" => 12,
        _ => return None,
    })
}

/// A count: digits, small number words, or "other" as in "every other day".
pub fn cardinal(word: &str) -> Option<u32> {
    if let Ok(n) = word.parse::<u32>() {
        return Some(n);
    }
    Some(match word {
        "one" => 1,
        "two" | "other" => 2,
        "three" => 3,
        "four" => 4,
        "five" => 5,
        "six" => 6,
        "seven" => 7,
        "eight" => 8,
        "nine" => 9,
        "ten" => 10,
        "eleven" => 11,
        "twelve" => 12,
        _ => return None,
    })
}

/// A day of the month written as an ordinal ("15th", "1st"). The suffix is
/// required: a bare "15" is too ambiguous to claim.
pub fn ordinal_day(word: &str) -> Option<u32> {
    let stem = word
        .strip_suffix("st")
        .or_else(|| word.strip_suffix("nd"))
        .or_else(|| word.strip_suffix("rd"))
        .or_else(|| word.strip_suffix("th"))?;
    match stem.parse::<u32>() {
        Ok(d) if (1..=31).contains(&d) => Some(d),
        _ => None,
    }
}

/// Strips an ordinal suffix if present, for "march 5th" style dates where a
/// bare number is already anchored by the month name.
pub fn day_number(word: &str) -> Option<u32> {
    let n = ordinal_day(word).or_else(|| word.parse::<u32>().ok())?;
    (1..=31).contains(&n).then_some(n)
}
