//! Table-driven corpus for the single-line parser.
//!
//! Every case runs against a fixed "now" — Sunday 2026-09-13 at 09:00 — so
//! relative phrases have one right answer. Sunday is deliberate: it makes the
//! difference between "sunday" (today) and "next sunday" (a week out) visible.

use chrono::{NaiveDate, NaiveDateTime, NaiveTime, Weekday};
use mtodo_core::{parse_at, Field, Recurrence};

fn now() -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, 9, 13).unwrap().and_hms_opt(9, 0, 0).unwrap()
}

fn date(y: i32, m: u32, d: u32) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(y, m, d)
}

fn time(h: u32, m: u32) -> Option<NaiveTime> {
    NaiveTime::from_hms_opt(h, m, 0)
}

/// One row: input, then the four fields it should produce.
struct Case {
    input: &'static str,
    title: &'static str,
    date: Option<NaiveDate>,
    time: Option<NaiveTime>,
    recurrence: Option<Recurrence>,
}

fn check(cases: &[Case]) {
    for case in cases {
        let result = parse_at(case.input, now());
        let task = &result.task;
        assert_eq!(task.title, case.title, "title for {:?}", case.input);
        assert_eq!(task.date, case.date, "date for {:?}", case.input);
        assert_eq!(task.time, case.time, "time for {:?}", case.input);
        assert_eq!(task.recurrence, case.recurrence, "recurrence for {:?}", case.input);
    }
}

/// Shorthand so the tables below stay readable.
macro_rules! case {
    ($input:expr => $title:expr $(, date: $d:expr)? $(, time: $t:expr)? $(, rec: $r:expr)? $(,)?) => {
        Case {
            input: $input,
            title: $title,
            date: None $(.or($d))?,
            time: None $(.or($t))?,
            recurrence: None $(.or(Some($r)))?,
        }
    };
}

#[test]
fn the_motivating_example() {
    let result = parse_at("gym every day 5pm", now());
    assert_eq!(result.task.title, "gym");
    assert_eq!(result.task.recurrence, Some(Recurrence::Daily));
    assert_eq!(result.task.time, time(17, 0));
    assert_eq!(result.task.date, None, "a recurrence rule is not a due date");
}

#[test]
fn plain_titles_stay_untouched() {
    check(&[
        case!("buy milk" => "buy milk"),
        case!("" => ""),
        case!("   " => ""),
        case!("call 5 people" => "call 5 people"),
        case!("read chapter 3" => "read chapter 3"),
        case!("Email Dana about Q4" => "Email Dana about Q4"),
    ]);
}

#[test]
fn times() {
    check(&[
        case!("standup 9am" => "standup", time: time(9, 0)),
        case!("standup at 9 am" => "standup", time: time(9, 0)),
        case!("lunch 12pm" => "lunch", time: time(12, 0)),
        case!("sleep 12am" => "sleep", time: time(0, 0)),
        case!("call 5:30pm" => "call", time: time(17, 30)),
        case!("deploy 17:00" => "deploy", time: time(17, 0)),
        case!("deploy at 17" => "deploy", time: time(17, 0)),
        case!("lunch at noon" => "lunch", time: time(12, 0)),
        case!("wake up at midnight" => "wake up", time: time(0, 0)),
        case!("call mom at 5" => "call mom", time: time(17, 0)),
        case!("workout at 7" => "workout", time: time(7, 0)),
    ]);
}

#[test]
fn a_bare_hour_is_only_a_time_when_marked() {
    // "at" licenses the guess; nothing else does.
    assert_eq!(parse_at("invite 6 friends", now()).task.time, None);
    assert_eq!(parse_at("invite friends at 6", now()).task.time, time(18, 0));
    // A guessed meridiem is recorded; a written one is not.
    assert!(parse_at("gym at 5", now()).is_guessed(Field::Time));
    assert!(!parse_at("gym at 5pm", now()).is_guessed(Field::Time));
    assert!(!parse_at("gym at 17", now()).is_guessed(Field::Time));
}

#[test]
fn relative_dates() {
    check(&[
        case!("pay rent today" => "pay rent", date: date(2026, 9, 13)),
        case!("pay rent tomorrow" => "pay rent", date: date(2026, 9, 14)),
        case!("submit form in 3 days" => "submit form", date: date(2026, 9, 16)),
        case!("submit form in two weeks" => "submit form", date: date(2026, 9, 27)),
        case!("renew passport in 2 months" => "renew passport", date: date(2026, 11, 13)),
        case!("review next week" => "review", date: date(2026, 9, 20)),
        case!("review next month" => "review", date: date(2026, 10, 13)),
        case!("dentist tonight" => "dentist", date: date(2026, 9, 13)),
    ]);
}

#[test]
fn weekday_dates() {
    check(&[
        // Today is Sunday, so a bare weekday can land on today.
        case!("brunch sunday" => "brunch", date: date(2026, 9, 13)),
        case!("brunch next sunday" => "brunch", date: date(2026, 9, 20)),
        case!("standup monday" => "standup", date: date(2026, 9, 14)),
        case!("standup next monday" => "standup", date: date(2026, 9, 14)),
        case!("drinks on friday" => "drinks", date: date(2026, 9, 18)),
        case!("drinks this friday" => "drinks", date: date(2026, 9, 18)),
        case!("sync wed" => "sync", date: date(2026, 9, 16)),
    ]);
}

#[test]
fn absolute_dates() {
    check(&[
        case!("taxes on 2027-04-15" => "taxes", date: date(2027, 4, 15)),
        case!("flight march 5" => "flight", date: date(2027, 3, 5)),
        case!("flight march 5 2029" => "flight", date: date(2029, 3, 5)),
        case!("flight 5th of march" => "flight", date: date(2027, 3, 5)),
        case!("conference oct 2nd" => "conference", date: date(2026, 10, 2)),
        case!("party on 25 dec" => "party", date: date(2026, 12, 25)),
    ]);
}

#[test]
fn a_past_month_day_rolls_to_next_year() {
    // March 5 has already gone by on 2026-09-13.
    let rolled = parse_at("flight march 5", now());
    assert_eq!(rolled.task.date, date(2027, 3, 5));
    assert!(rolled.is_guessed(Field::Date));

    // December 25 has not.
    let same_year = parse_at("party dec 25", now());
    assert_eq!(same_year.task.date, date(2026, 12, 25));
    assert!(!same_year.is_guessed(Field::Date));
}

#[test]
fn recurrence_rules() {
    check(&[
        case!("gym every day" => "gym", rec: Recurrence::Daily),
        case!("gym daily" => "gym", rec: Recurrence::Daily),
        case!("water plants every other day" => "water plants", rec: Recurrence::EveryNDays(2)),
        case!("bins every 3 days" => "bins", rec: Recurrence::EveryNDays(3)),
        case!("standup every monday" => "standup",
              rec: Recurrence::Weekly { weekday: Some(Weekday::Mon) }),
        case!("standup mondays" => "standup",
              rec: Recurrence::Weekly { weekday: Some(Weekday::Mon) }),
        case!("review every week" => "review", rec: Recurrence::Weekly { weekday: None }),
        case!("payday every other friday" => "payday",
              rec: Recurrence::EveryNWeeks { n: 2, weekday: Some(Weekday::Fri) }),
        case!("retro biweekly" => "retro",
              rec: Recurrence::EveryNWeeks { n: 2, weekday: None }),
        case!("standup every weekday" => "standup", rec: Recurrence::Weekdays),
        case!("laundry every weekend" => "laundry", rec: Recurrence::Weekends),
        case!("rent every month" => "rent", rec: Recurrence::Monthly { day: None }),
        case!("rent every 15th" => "rent", rec: Recurrence::Monthly { day: Some(15) }),
        case!("dentist every 6 months" => "dentist", rec: Recurrence::EveryNMonths(6)),
        case!("mot every year" => "mot", rec: Recurrence::Yearly),
        // "every 1 week" collapses onto the plain weekly rule.
        case!("sync every 1 week" => "sync", rec: Recurrence::Weekly { weekday: None }),
    ]);
}

#[test]
fn recurrence_claims_the_weekday_before_the_date_extractor_sees_it() {
    let result = parse_at("standup every monday 9am", now());
    assert_eq!(result.task.title, "standup");
    assert_eq!(result.task.recurrence, Some(Recurrence::Weekly { weekday: Some(Weekday::Mon) }));
    assert_eq!(result.task.time, time(9, 0));
    assert_eq!(result.task.date, None);
}

#[test]
fn combinations() {
    check(&[
        case!("call mom tomorrow at 5" => "call mom",
              date: date(2026, 9, 14), time: time(17, 0)),
        case!("team sync every monday at 9:30" => "team sync",
              time: time(9, 30), rec: Recurrence::Weekly { weekday: Some(Weekday::Mon) }),
        case!("dentist on 2027-04-15 at 2pm" => "dentist",
              date: date(2027, 4, 15), time: time(14, 0)),
        case!("water plants every other day at 8am" => "water plants",
              time: time(8, 0), rec: Recurrence::EveryNDays(2)),
    ]);
}

#[test]
fn anchored_recurrence() {
    check(&[
        case!("pay rent every month on the 1st" => "pay rent",
              rec: Recurrence::Monthly { day: Some(1) }),
        case!("review every week on friday" => "review",
              rec: Recurrence::Weekly { weekday: Some(Weekday::Fri) }),
        case!("sync every 2 weeks on monday" => "sync",
              rec: Recurrence::EveryNWeeks { n: 2, weekday: Some(Weekday::Mon) }),
    ]);
}

#[test]
fn an_abbreviation_ending_in_s_is_not_a_recurrence() {
    // "tues" is singular Tuesday; only a full name pluralizes into a rule.
    check(&[
        case!("sync tues" => "sync", date: date(2026, 9, 15)),
        case!("sync weds" => "sync", date: date(2026, 9, 16)),
        case!("sync tuesdays" => "sync",
              rec: Recurrence::Weekly { weekday: Some(Weekday::Tue) }),
    ]);
}

#[test]
fn an_extractor_never_reads_a_token_another_one_claimed() {
    // Before word_at, the recurrence pass took "tues" and the date pass still
    // read it through "next", producing both a rule and a date from one word.
    let result = parse_at("lunch with sam next tues 12:30pm", now());
    assert_eq!(result.task.title, "lunch with sam");
    assert_eq!(result.task.date, date(2026, 9, 15));
    assert_eq!(result.task.time, time(12, 30));
    assert_eq!(result.task.recurrence, None);
}

#[test]
fn matches_point_back_at_the_input() {
    let input = "gym every day 5pm";
    let result = parse_at(input, now());

    let rec = result.matched(Field::Recurrence).expect("recurrence match");
    assert_eq!(rec.text, "every day");
    assert_eq!(&input[rec.span.clone()], "every day");

    let time = result.matched(Field::Time).expect("time match");
    assert_eq!(time.text, "5pm");
    assert_eq!(&input[time.span.clone()], "5pm");

    assert!(result.matched(Field::Date).is_none());
}

#[test]
fn punctuation_and_spacing_survive() {
    check(&[
        case!("  gym   every day   5pm  " => "gym", time: time(17, 0), rec: Recurrence::Daily),
        case!("call mom, tomorrow" => "call mom", date: date(2026, 9, 14)),
        case!("gym every day 5 p.m." => "gym", time: time(17, 0), rec: Recurrence::Daily),
    ]);
}
