//! Table-driven corpus for the single-line parser.
//!
//! Every case runs against a fixed "now" — Sunday 2026-09-13 at 09:00 — so
//! relative phrases have one right answer. Sunday is deliberate: it makes the
//! difference between "sunday" (today) and "next sunday" (a week out) visible.

use chrono::{NaiveDate, NaiveDateTime, NaiveTime, Weekday};
use kairos_core::{parse_at, Field, Priority, Recurrence, WeekdaySet};

fn now() -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, 9, 13).unwrap().and_hms_opt(9, 0, 0).unwrap()
}

fn date(y: i32, m: u32, d: u32) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(y, m, d)
}

/// The anchor a recurring task gets when the input names no start date.
fn today() -> Option<NaiveDate> {
    date(2026, 9, 13)
}

fn tomorrow() -> Option<NaiveDate> {
    date(2026, 9, 14)
}

fn time(h: u32, m: u32) -> Option<NaiveTime> {
    NaiveTime::from_hms_opt(h, m, 0)
}

fn weekly(days: &[Weekday]) -> Recurrence {
    Recurrence::Weekly { days: days.iter().copied().collect() }
}

/// One row: input, then the fields it should produce.
struct Case {
    input: &'static str,
    title: &'static str,
    due: Option<NaiveDate>,
    time: Option<NaiveTime>,
    recurrence: Option<Recurrence>,
}

fn check(cases: &[Case]) {
    for case in cases {
        let result = parse_at(case.input, now());
        let task = &result.task;
        assert_eq!(task.title, case.title, "title for {:?}", case.input);
        assert_eq!(task.due, case.due, "due for {:?}", case.input);
        assert_eq!(task.time, case.time, "time for {:?}", case.input);
        assert_eq!(task.recurrence, case.recurrence, "recurrence for {:?}", case.input);
    }
}

/// Shorthand so the tables below stay readable.
macro_rules! case {
    ($input:expr => $title:expr $(, due: $d:expr)? $(, time: $t:expr)? $(, rec: $r:expr)? $(,)?) => {
        Case {
            input: $input,
            title: $title,
            due: None $(.or($d))?,
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
    // No start date was named, so the series is anchored to today.
    assert_eq!(result.task.due, today());
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
        case!("ship it!" => "ship it!"),
    ]);
}

#[test]
fn times() {
    // "Now" is 09:00, so a time later today lands today and an earlier one
    // rolls to tomorrow — see `a_bare_time_means_the_next_time_it_comes_round`.
    check(&[
        case!("lunch 12pm" => "lunch", due: today(), time: time(12, 0)),
        case!("call 5:30pm" => "call", due: today(), time: time(17, 30)),
        case!("deploy 17:00" => "deploy", due: today(), time: time(17, 0)),
        case!("deploy at 17" => "deploy", due: today(), time: time(17, 0)),
        case!("lunch at noon" => "lunch", due: today(), time: time(12, 0)),
        case!("call mom at 5" => "call mom", due: today(), time: time(17, 0)),
        case!("standup 9am" => "standup", due: tomorrow(), time: time(9, 0)),
        case!("standup at 9 am" => "standup", due: tomorrow(), time: time(9, 0)),
        case!("sleep 12am" => "sleep", due: tomorrow(), time: time(0, 0)),
        case!("wake up at midnight" => "wake up", due: tomorrow(), time: time(0, 0)),
        case!("workout at 7" => "workout", due: tomorrow(), time: time(7, 0)),
    ]);
}

#[test]
fn a_bare_time_means_the_next_time_it_comes_round() {
    // A time with no date used to produce a dateless task: no view showed it and
    // no reminder could fire. It now lands on the next day that clock time
    // arrives, and says so when that is not today.
    let later = parse_at("take the bread out at 5pm", now());
    assert_eq!(later.task.due, today());
    assert!(!later.is_guessed(Field::Date));

    let passed = parse_at("take the bread out at 7am", now());
    assert_eq!(passed.task.due, tomorrow());
    assert!(passed.is_guessed(Field::Date), "rolling to tomorrow is an inference");

    // An explicit date still wins.
    let dated = parse_at("call mom friday at 7am", now());
    assert_eq!(dated.task.due, date(2026, 9, 18));
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
        case!("pay rent today" => "pay rent", due: date(2026, 9, 13)),
        case!("pay rent tomorrow" => "pay rent", due: date(2026, 9, 14)),
        case!("submit form in 3 days" => "submit form", due: date(2026, 9, 16)),
        case!("submit form in two weeks" => "submit form", due: date(2026, 9, 27)),
        case!("renew passport in 2 months" => "renew passport", due: date(2026, 11, 13)),
        case!("review next week" => "review", due: date(2026, 9, 20)),
        case!("review next month" => "review", due: date(2026, 10, 13)),
        case!("dentist tonight" => "dentist", due: date(2026, 9, 13)),
    ]);
}

#[test]
fn weekday_dates() {
    check(&[
        // Today is Sunday, so a bare weekday can land on today.
        case!("brunch sunday" => "brunch", due: date(2026, 9, 13)),
        case!("brunch next sunday" => "brunch", due: date(2026, 9, 20)),
        case!("standup monday" => "standup", due: date(2026, 9, 14)),
        case!("standup next monday" => "standup", due: date(2026, 9, 14)),
        case!("drinks on friday" => "drinks", due: date(2026, 9, 18)),
        case!("drinks this friday" => "drinks", due: date(2026, 9, 18)),
        case!("sync wed" => "sync", due: date(2026, 9, 16)),
    ]);
}

#[test]
fn absolute_dates() {
    check(&[
        case!("taxes on 2027-04-15" => "taxes", due: date(2027, 4, 15)),
        case!("flight march 5" => "flight", due: date(2027, 3, 5)),
        case!("flight march 5 2029" => "flight", due: date(2029, 3, 5)),
        case!("flight 5th of march" => "flight", due: date(2027, 3, 5)),
        case!("conference oct 2nd" => "conference", due: date(2026, 10, 2)),
        case!("party on 25 dec" => "party", due: date(2026, 12, 25)),
    ]);
}

#[test]
fn a_past_month_day_rolls_to_next_year() {
    // March 5 has already gone by on 2026-09-13.
    let rolled = parse_at("flight march 5", now());
    assert_eq!(rolled.task.due, date(2027, 3, 5));
    assert!(rolled.is_guessed(Field::Date));

    // December 25 has not.
    let same_year = parse_at("party dec 25", now());
    assert_eq!(same_year.task.due, date(2026, 12, 25));
    assert!(!same_year.is_guessed(Field::Date));
}

#[test]
fn recurrence_rules() {
    check(&[
        case!("gym every day" => "gym", due: today(), rec: Recurrence::Daily),
        case!("gym daily" => "gym", due: today(), rec: Recurrence::Daily),
        case!("water plants every other day" => "water plants",
              due: today(), rec: Recurrence::EveryNDays(2)),
        case!("bins every 3 days" => "bins", due: today(), rec: Recurrence::EveryNDays(3)),
        case!("standup every monday" => "standup",
              due: today(), rec: weekly(&[Weekday::Mon])),
        case!("standup mondays" => "standup", due: today(), rec: weekly(&[Weekday::Mon])),
        case!("review every week" => "review",
              due: today(), rec: Recurrence::Weekly { days: WeekdaySet::EMPTY }),
        case!("payday every other friday" => "payday", due: today(),
              rec: Recurrence::EveryNWeeks { n: 2, days: WeekdaySet::from_day(Weekday::Fri) }),
        case!("retro biweekly" => "retro", due: today(),
              rec: Recurrence::EveryNWeeks { n: 2, days: WeekdaySet::EMPTY }),
        case!("standup every weekday" => "standup", due: today(), rec: Recurrence::WEEKDAYS),
        case!("laundry every weekend" => "laundry", due: today(), rec: Recurrence::WEEKENDS),
        case!("rent every month" => "rent",
              due: today(), rec: Recurrence::Monthly { day: None }),
        case!("rent every 15th" => "rent",
              due: today(), rec: Recurrence::Monthly { day: Some(15) }),
        case!("dentist every 6 months" => "dentist",
              due: today(), rec: Recurrence::EveryNMonths(6)),
        case!("mot every year" => "mot", due: today(), rec: Recurrence::Yearly),
        // "every 1 week" collapses onto the plain weekly rule.
        case!("sync every 1 week" => "sync",
              due: today(), rec: Recurrence::Weekly { days: WeekdaySet::EMPTY }),
    ]);
}

#[test]
fn multi_weekday_rules() {
    check(&[
        case!("gym every monday and wednesday" => "gym",
              due: today(), rec: weekly(&[Weekday::Mon, Weekday::Wed])),
        // The tokenizer strips the commas, so the list form reads the same way.
        case!("gym every monday, wednesday and friday" => "gym",
              due: today(), rec: weekly(&[Weekday::Mon, Weekday::Wed, Weekday::Fri])),
        case!("standup mondays and thursdays" => "standup",
              due: today(), rec: weekly(&[Weekday::Mon, Weekday::Thu])),
        case!("review every week on tuesday and friday" => "review",
              due: today(), rec: weekly(&[Weekday::Tue, Weekday::Fri])),
    ]);
}

#[test]
fn a_weekday_after_the_rule_is_not_absorbed_blindly() {
    // "gym" breaks the run, so "friday" stays a due date rather than joining
    // the weekly set.
    let result = parse_at("every monday gym friday", now());
    assert_eq!(result.task.recurrence, Some(weekly(&[Weekday::Mon])));
    assert_eq!(result.task.title, "gym");
    assert_eq!(result.task.due, date(2026, 9, 18));
}

#[test]
fn anchored_recurrence() {
    check(&[
        case!("pay rent every month on the 1st" => "pay rent",
              due: today(), rec: Recurrence::Monthly { day: Some(1) }),
        case!("review every week on friday" => "review",
              due: today(), rec: weekly(&[Weekday::Fri])),
        case!("sync every 2 weeks on monday" => "sync", due: today(),
              rec: Recurrence::EveryNWeeks { n: 2, days: WeekdaySet::from_day(Weekday::Mon) }),
    ]);
}

#[test]
fn an_abbreviation_ending_in_s_is_not_a_recurrence() {
    // "tues" is singular Tuesday; only a full name pluralizes into a rule.
    check(&[
        case!("sync tues" => "sync", due: date(2026, 9, 15)),
        case!("sync weds" => "sync", due: date(2026, 9, 16)),
        case!("sync tuesdays" => "sync", due: today(), rec: weekly(&[Weekday::Tue])),
    ]);
}

#[test]
fn an_extractor_never_reads_a_token_another_one_claimed() {
    // Before word_at, the recurrence pass took "tues" and the date pass still
    // read it through "next", producing both a rule and a date from one word.
    let result = parse_at("lunch with sam next tues 12:30pm", now());
    assert_eq!(result.task.title, "lunch with sam");
    assert_eq!(result.task.due, date(2026, 9, 15));
    assert_eq!(result.task.time, time(12, 30));
    assert_eq!(result.task.recurrence, None);
}

#[test]
fn recurrence_claims_the_weekday_before_the_date_extractor_sees_it() {
    let result = parse_at("standup every monday 9am", now());
    assert_eq!(result.task.title, "standup");
    assert_eq!(result.task.recurrence, Some(weekly(&[Weekday::Mon])));
    assert_eq!(result.task.time, time(9, 0));
    assert_eq!(result.task.due, today());
}

#[test]
fn tags() {
    let result = parse_at("buy milk #errands #home", now());
    assert_eq!(result.task.title, "buy milk");
    assert_eq!(result.task.tags, ["errands", "home"]);

    // Tags are lowercased and deduplicated.
    let dupes = parse_at("call mom #Family #family", now());
    assert_eq!(dupes.task.tags, ["family"]);
}

#[test]
fn projects_and_priorities() {
    let result = parse_at("draft spec @work !p1", now());
    assert_eq!(result.task.title, "draft spec");
    assert_eq!(result.task.project.as_deref(), Some("work"));
    assert_eq!(result.task.priority, Priority::High);

    // Bang runs read as a whole: "!!!" is high, not medium.
    assert_eq!(parse_at("fix build !!!", now()).task.priority, Priority::High);
    assert_eq!(parse_at("fix build !!", now()).task.priority, Priority::Medium);
    assert_eq!(parse_at("fix build !", now()).task.priority, Priority::Low);
    assert_eq!(parse_at("fix build !high", now()).task.priority, Priority::High);

    // A trailing bang on a word is punctuation, not a priority.
    assert_eq!(parse_at("ship it!", now()).task.priority, Priority::None);
}

#[test]
fn a_sigil_beats_the_date_and_recurrence_readings() {
    // "#friday" is a tag, so no due date should come out of it.
    let tagged = parse_at("retro #friday", now());
    assert_eq!(tagged.task.tags, ["friday"]);
    assert_eq!(tagged.task.due, None);

    // Same for a project that happens to be named after a month.
    let project = parse_at("plan launch @march", now());
    assert_eq!(project.task.project.as_deref(), Some("march"));
    assert_eq!(project.task.due, None);
}

#[test]
fn everything_at_once() {
    let result = parse_at("submit report every monday 9am @work #urgent !p1", now());
    let task = &result.task;
    assert_eq!(task.title, "submit report");
    assert_eq!(task.recurrence, Some(weekly(&[Weekday::Mon])));
    assert_eq!(task.time, time(9, 0));
    assert_eq!(task.project.as_deref(), Some("work"));
    assert_eq!(task.tags, ["urgent"]);
    assert_eq!(task.priority, Priority::High);
}

#[test]
fn combinations() {
    check(&[
        case!("call mom tomorrow at 5" => "call mom",
              due: date(2026, 9, 14), time: time(17, 0)),
        case!("team sync every monday at 9:30" => "team sync",
              due: today(), time: time(9, 30), rec: weekly(&[Weekday::Mon])),
        case!("dentist on 2027-04-15 at 2pm" => "dentist",
              due: date(2027, 4, 15), time: time(14, 0)),
        case!("water plants every other day at 8am" => "water plants",
              due: today(), time: time(8, 0), rec: Recurrence::EveryNDays(2)),
    ]);
}

#[test]
fn a_named_start_date_anchors_the_series() {
    // An explicit date wins over the today default.
    let result = parse_at("gym every day starting tomorrow", now());
    assert_eq!(result.task.recurrence, Some(Recurrence::Daily));
    assert_eq!(result.task.due, date(2026, 9, 14));
}

#[test]
fn a_parse_can_be_reverted_to_plain_text() {
    // The capture field highlights what it recognized; pressing backspace on a
    // highlight hands its range back as excluded, and the words stay in the
    // title instead of becoming a field.
    let input = "write about my monday every week";

    let parsed = parse_at(input, now());
    assert_eq!(parsed.task.title, "write about my");
    assert_eq!(parsed.task.due, date(2026, 9, 14));
    let monday = parsed.matched(Field::Date).expect("a date match").span.clone();
    assert_eq!(&input[monday.clone()], "monday");

    // Revert just the date; the recurrence it sat beside is untouched.
    let reverted = kairos_core::parse_excluding(input, now(), &[monday]);
    assert_eq!(reverted.task.title, "write about my monday");
    assert_eq!(reverted.task.recurrence, Some(Recurrence::Weekly { days: WeekdaySet::EMPTY }));
    assert!(reverted.matched(Field::Date).is_none());
}

#[test]
fn reverting_every_span_leaves_the_line_exactly_as_typed() {
    let input = "gym every day 5pm";
    let parsed = parse_at(input, now());
    let spans: Vec<_> = parsed.matches.iter().map(|m| m.span.clone()).collect();

    let reverted = kairos_core::parse_excluding(input, now(), &spans);
    assert_eq!(reverted.task.title, input);
    assert_eq!(reverted.task.recurrence, None);
    assert_eq!(reverted.task.time, None);
    assert_eq!(reverted.task.due, None);
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
        case!("  gym   every day   5pm  " => "gym",
              due: today(), time: time(17, 0), rec: Recurrence::Daily),
        case!("call mom, tomorrow" => "call mom", due: date(2026, 9, 14)),
        case!("gym every day 5 p.m." => "gym",
              due: today(), time: time(17, 0), rec: Recurrence::Daily),
    ]);
}

/// Days written with slashes, the way a gym plan or a calendar writes them.
#[test]
fn weekday_runs_may_use_slashes() {
    use kairos_core::recurrence_from_phrase;
    let expected = recurrence_from_phrase("every monday and wednesday and friday");
    assert_eq!(recurrence_from_phrase("every mon/wed/fri"), expected);
    assert_eq!(recurrence_from_phrase("every monday/wednesday/friday"), expected);
    // A slash between things that are not all weekdays is not a weekday run.
    assert_eq!(recurrence_from_phrase("every mon/nonsense"), None);
}
