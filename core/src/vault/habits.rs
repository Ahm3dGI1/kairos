//! Habits, the day's two numbers, and the month's journal as Markdown.
//!
//! Split in two, because they change at different rates. `habits/habits.md`
//! defines the habits once; `habits/2026-09.md` holds what actually happened
//! that month:
//!
//! ```text
//! # September 2026
//!
//! ## Ticks
//!
//! - Gym: 1 3 5 8 10 12
//! - Read: 2 3 4
//!
//! ## Days
//!
//! | Day | Screen | Sleep |
//! | --- | --- | --- |
//! | 1 | 3h 20m | 7h 30m |
//!
//! ## Journal
//!
//! ### Week one
//! Text under the heading.
//! ```
//!
//! Ticks name their habit rather than carrying an id, so adding
//! `- Swim: 5 6 7` to a month file is all it takes to start tracking one.

use std::collections::HashMap;

use chrono::{Datelike, NaiveDate};
use uuid::Uuid;

use crate::daily::{parse_duration, DayLog, Habit, HabitId, JournalEntry, MonthJournal};

const DATE: &str = "%Y-%m-%d";

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

// ---------------------------------------------------------------- definitions

/// Renders `habits/habits.md`.
pub fn write_habits(habits: &[Habit]) -> String {
    let mut out = String::from("# Habits\n\n");
    for habit in habits.iter().filter(|h| !h.archived) {
        out.push_str(&habit_line(habit));
    }
    let archived: Vec<&Habit> = habits.iter().filter(|h| h.archived).collect();
    if !archived.is_empty() {
        out.push_str("\n## Archived\n\n");
        for habit in archived {
            out.push_str(&habit_line(habit));
        }
    }
    out
}

fn habit_line(habit: &Habit) -> String {
    format!(
        "- {} created:{} ^{}\n",
        habit.name.replace(['\n', ':'], " ").trim(),
        habit.created_at.format(DATE),
        habit.id
    )
}

/// Reads `habits/habits.md`. Position comes from the order in the file, so
/// reordering the list is a matter of moving a line.
pub fn read_habits(text: &str, today: NaiveDate) -> Vec<Habit> {
    let mut habits = Vec::new();
    let mut archived = false;
    for line in text.lines() {
        let line = line.trim();
        if let Some(heading) = line.strip_prefix("## ") {
            archived = heading.trim().eq_ignore_ascii_case("archived");
            continue;
        }
        let Some(rest) = line.strip_prefix("- ") else { continue };

        let mut name_parts = Vec::new();
        let mut id = None;
        let mut created = today;
        for token in rest.split_whitespace() {
            if let Some(raw) = token.strip_prefix('^') {
                if let Ok(parsed) = Uuid::parse_str(raw) {
                    id = Some(parsed);
                    continue;
                }
            }
            if let Some(raw) = token.strip_prefix("created:") {
                if let Ok(parsed) = NaiveDate::parse_from_str(raw, DATE) {
                    created = parsed;
                    continue;
                }
            }
            name_parts.push(token);
        }

        let name = name_parts.join(" ");
        if name.is_empty() {
            continue;
        }
        habits.push(Habit {
            id: id.unwrap_or_else(Uuid::new_v4),
            name,
            created_at: created,
            archived,
            position: habits.len() as i64,
        });
    }
    habits
}

// --------------------------------------------------------------------- months

/// What one month file holds.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Month {
    pub logs: Vec<DayLog>,
    pub journal: Vec<JournalEntry>,
    /// Habits named in the ticks that no definition file knew about — the
    /// caller creates them.
    pub unknown_habits: Vec<String>,
}

/// Renders one month file. `names` resolves the habit ids in `logs`.
pub fn write_month(
    year: i32,
    month: u32,
    habits: &[Habit],
    logs: &[DayLog],
    journal: &MonthJournal,
) -> String {
    let mut out = format!("# {} {year}\n", MONTHS[(month as usize - 1).min(11)]);

    let mut ticks: Vec<(String, Vec<u32>)> = Vec::new();
    for habit in habits {
        let days: Vec<u32> =
            logs.iter().filter(|log| log.is_done(habit.id)).map(|log| log.date.day()).collect();
        if !days.is_empty() {
            ticks.push((habit.name.clone(), days));
        }
    }
    if !ticks.is_empty() {
        out.push_str("\n## Ticks\n\n");
        for (name, mut days) in ticks {
            days.sort_unstable();
            let days: Vec<String> = days.iter().map(u32::to_string).collect();
            out.push_str(&format!("- {name}: {}\n", days.join(" ")));
        }
    }

    let measured: Vec<&DayLog> = logs
        .iter()
        .filter(|log| log.screen_minutes.is_some() || log.sleep_minutes.is_some())
        .collect();
    if !measured.is_empty() {
        out.push_str("\n## Days\n\n| Day | Screen | Sleep |\n| --- | --- | --- |\n");
        let mut measured = measured;
        measured.sort_by_key(|log| log.date);
        for log in measured {
            out.push_str(&format!(
                "| {} | {} | {} |\n",
                log.date.day(),
                log.screen_minutes.map(crate::daily::format_duration).unwrap_or_default(),
                log.sleep_minutes.map(crate::daily::format_duration).unwrap_or_default(),
            ));
        }
    }

    if !journal.entries.is_empty() {
        out.push_str("\n## Journal\n");
        for entry in &journal.entries {
            out.push_str(&format!("\n### {}\n", entry.title.replace('\n', " ").trim()));
            if !entry.body.trim().is_empty() {
                out.push_str(entry.body.trim_end());
                out.push('\n');
            }
        }
    }

    out
}

/// Reads one month file. `known` maps habit name (lowercased) to its id.
pub fn read_month(text: &str, year: i32, month: u32, known: &HashMap<String, HabitId>) -> Month {
    let mut result = Month::default();
    let mut logs: HashMap<u32, DayLog> = HashMap::new();
    let mut section = Section::None;
    let mut entry: Option<JournalEntry> = None;
    let mut body = String::new();

    let day_of = |day: u32| NaiveDate::from_ymd_opt(year, month, day);

    for line in text.lines() {
        let trimmed = line.trim();

        if let Some(heading) = trimmed.strip_prefix("## ") {
            flush(&mut entry, &mut body, &mut result.journal);
            section = match heading.trim().to_lowercase().as_str() {
                "ticks" | "habits" => Section::Ticks,
                "days" | "metrics" => Section::Days,
                "journal" => Section::Journal,
                _ => Section::None,
            };
            continue;
        }

        if section == Section::Journal {
            if let Some(title) = trimmed.strip_prefix("### ") {
                flush(&mut entry, &mut body, &mut result.journal);
                entry = Some(JournalEntry::new(title.trim()));
                continue;
            }
            if entry.is_some() {
                body.push_str(line);
                body.push('\n');
            }
            continue;
        }

        match section {
            Section::Ticks => {
                let Some(rest) = trimmed.strip_prefix("- ") else { continue };
                let Some((name, days)) = rest.split_once(':') else { continue };
                let name = name.trim();
                if name.is_empty() {
                    continue;
                }
                let id = match known.get(&name.to_lowercase()) {
                    Some(id) => *id,
                    None => {
                        if !result.unknown_habits.iter().any(|n| n.eq_ignore_ascii_case(name)) {
                            result.unknown_habits.push(name.to_string());
                        }
                        // A habit the caller is about to create needs a stable
                        // id now, so the ticks can point at it.
                        derived_habit_id(name)
                    }
                };
                for token in days.split([' ', ',', '\t']) {
                    let Ok(day) = token.trim().parse::<u32>() else { continue };
                    let Some(date) = day_of(day) else { continue };
                    let log = logs.entry(day).or_insert_with(|| DayLog::new(date));
                    if !log.is_done(id) {
                        log.habits_done.push(id);
                    }
                }
            }
            Section::Days => {
                let Some(row) = table_row(trimmed) else { continue };
                if row.len() < 2 {
                    continue;
                }
                let Ok(day) = row[0].parse::<u32>() else { continue };
                let Some(date) = day_of(day) else { continue };
                let log = logs.entry(day).or_insert_with(|| DayLog::new(date));
                log.screen_minutes = row.get(1).and_then(|v| parse_duration(v));
                log.sleep_minutes = row.get(2).and_then(|v| parse_duration(v));
            }
            _ => {}
        }
    }
    flush(&mut entry, &mut body, &mut result.journal);

    let mut logs: Vec<DayLog> = logs.into_values().collect();
    logs.sort_by_key(|log| log.date);
    result.logs = logs;
    result
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum Section {
    None,
    Ticks,
    Days,
    Journal,
}

fn flush(entry: &mut Option<JournalEntry>, body: &mut String, into: &mut Vec<JournalEntry>) {
    if let Some(mut entry) = entry.take() {
        entry.body = body.trim_matches('\n').to_string();
        if !entry.is_empty() {
            into.push(entry);
        }
    }
    body.clear();
}

/// Splits `| a | b | c |` into its cells, skipping the `| --- |` rule.
fn table_row(line: &str) -> Option<Vec<String>> {
    let inner = line.strip_prefix('|')?.strip_suffix('|').unwrap_or(line.strip_prefix('|')?);
    let cells: Vec<String> = inner.split('|').map(|c| c.trim().to_string()).collect();
    if cells.iter().all(|c| c.chars().all(|ch| ch == '-' || ch == ':') && !c.is_empty()) {
        return None;
    }
    Some(cells)
}

/// Namespace for habits invented by a month file, so the same name written into
/// two different months resolves to one habit.
const HABIT_NS: Uuid = Uuid::from_bytes([
    0x6d, 0x74, 0x6f, 0x64, 0x6f, 0x2d, 0x68, 0x61, 0x62, 0x69, 0x74, 0x2d, 0x6e, 0x65, 0x77, 0x31,
]);

pub fn derived_habit_id(name: &str) -> HabitId {
    Uuid::new_v5(&HABIT_NS, name.to_lowercase().as_bytes())
}

/// "2026-09" from a month filename, if it is one.
pub fn month_of(filename: &str) -> Option<(i32, u32)> {
    let stem = filename.strip_suffix(".md").unwrap_or(filename);
    let (year, month) = stem.split_once('-')?;
    let year: i32 = year.parse().ok()?;
    let month: u32 = month.parse().ok()?;
    (1..=12).contains(&month).then_some((year, month))
}

pub fn month_filename(year: i32, month: u32) -> String {
    format!("{year:04}-{month:02}.md")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, day).unwrap()
    }

    #[test]
    fn habit_definitions_round_trip() {
        let mut gym = Habit::new("Gym", date(1), 0);
        let mut read = Habit::new("Read", date(2), 1);
        read.archived = true;
        let text = write_habits(&[gym.clone(), read.clone()]);
        let back = read_habits(&text, date(20));

        assert_eq!(back.len(), 2);
        assert_eq!(back[0].id, gym.id);
        assert_eq!(back[0].name, "Gym");
        assert!(!back[0].archived);
        assert_eq!(back[1].id, read.id);
        assert!(back[1].archived, "the Archived heading is what marks it");

        gym.position = 0;
        read.position = 1;
        assert_eq!(back[0].created_at, gym.created_at);
    }

    #[test]
    fn a_month_round_trips() {
        let gym = Habit::new("Gym", date(1), 0);
        let read = Habit::new("Read", date(1), 1);

        let mut logs = vec![DayLog::new(date(1)), DayLog::new(date(3)), DayLog::new(date(5))];
        logs[0].habits_done.push(gym.id);
        logs[0].screen_minutes = Some(200);
        logs[0].sleep_minutes = Some(450);
        logs[1].habits_done.push(gym.id);
        logs[1].habits_done.push(read.id);
        logs[2].sleep_minutes = Some(400);

        let mut journal = MonthJournal::new(2026, 9);
        let mut entry = JournalEntry::new("Week one");
        entry.body = "It went fine.\n\nMostly.".into();
        journal.entries.push(entry);

        let text = write_month(2026, 9, &[gym.clone(), read.clone()], &logs, &journal);

        let known = HashMap::from([("gym".to_string(), gym.id), ("read".to_string(), read.id)]);
        let back = read_month(&text, 2026, 9, &known);

        assert!(back.unknown_habits.is_empty());
        assert_eq!(back.logs.len(), 3);
        assert_eq!(back.logs[0].date, date(1));
        assert!(back.logs[0].is_done(gym.id));
        assert_eq!(back.logs[0].screen_minutes, Some(200));
        assert_eq!(back.logs[0].sleep_minutes, Some(450));
        assert!(back.logs[1].is_done(read.id));
        assert_eq!(back.logs[2].sleep_minutes, Some(400));
        assert_eq!(back.logs[2].screen_minutes, None);

        assert_eq!(back.journal.len(), 1);
        assert_eq!(back.journal[0].title, "Week one");
        assert_eq!(back.journal[0].body, "It went fine.\n\nMostly.");
    }

    /// The reason ticks name their habit instead of carrying an id.
    #[test]
    fn a_habit_named_only_in_a_month_file_is_reported_as_new() {
        let text = "# September 2026\n\n## Ticks\n\n- Swim: 5 6 7\n";
        let back = read_month(text, 2026, 9, &HashMap::new());
        assert_eq!(back.unknown_habits, ["Swim"]);
        assert_eq!(back.logs.len(), 3);
        let id = derived_habit_id("Swim");
        assert!(back.logs.iter().all(|log| log.is_done(id)));
    }

    #[test]
    fn the_same_new_name_gets_the_same_id_in_every_month() {
        assert_eq!(derived_habit_id("Swim"), derived_habit_id("swim"));
        assert_ne!(derived_habit_id("Swim"), derived_habit_id("Run"));
    }

    #[test]
    fn ticks_may_be_comma_separated_too() {
        let text = "## Ticks\n\n- Gym: 1, 3, 5\n";
        let back = read_month(text, 2026, 9, &HashMap::new());
        assert_eq!(back.logs.len(), 3);
    }

    #[test]
    fn a_day_outside_the_month_is_ignored_rather_than_crashing() {
        let text = "## Ticks\n\n- Gym: 31 99\n";
        // September has 30 days.
        let back = read_month(text, 2026, 9, &HashMap::new());
        assert!(back.logs.is_empty());
    }

    #[test]
    fn month_filenames_parse() {
        assert_eq!(month_of("2026-09.md"), Some((2026, 9)));
        assert_eq!(month_of("2026-09"), Some((2026, 9)));
        assert_eq!(month_of("habits.md"), None);
        assert_eq!(month_of("2026-13.md"), None);
        assert_eq!(month_filename(2026, 9), "2026-09.md");
    }

    #[test]
    fn writing_a_month_twice_gives_the_same_text() {
        let gym = Habit::new("Gym", date(1), 0);
        let mut log = DayLog::new(date(2));
        log.habits_done.push(gym.id);
        log.screen_minutes = Some(90);
        let journal = MonthJournal::new(2026, 9);
        let once =
            write_month(2026, 9, std::slice::from_ref(&gym), std::slice::from_ref(&log), &journal);
        let known = HashMap::from([("gym".to_string(), gym.id)]);
        let back = read_month(&once, 2026, 9, &known);
        let twice = write_month(2026, 9, &[gym], &back.logs, &journal);
        assert_eq!(once, twice);
    }
}
