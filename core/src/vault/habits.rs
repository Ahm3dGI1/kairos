use std::collections::HashMap;

use chrono::{Datelike, NaiveDate};
use uuid::Uuid;

use crate::daily::{DayLog, Habit, HabitId, HabitKind, JournalEntry, MonthJournal};

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

// -- definitions
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
    let kind = match &habit.kind {
        // A tick is the default, so saying so would only be noise.
        HabitKind::Check => String::new(),
        other => format!(" kind:{}", other.label()),
    };
    format!(
        "- {}{kind} created:{} ^{}\n",
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
        let mut kind = HabitKind::Check;
        for token in rest.split_whitespace() {
            if let Some(raw) = token.strip_prefix("kind:") {
                kind = HabitKind::parse(raw);
                continue;
            }
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
            kind,
            created_at: created,
            archived,
            position: habits.len() as i64,
        });
    }
    habits
}

// -- months
/// What one month file holds.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Month {
    pub logs: Vec<DayLog>,
    pub journal: Vec<JournalEntry>,
    /// Habits named in this file that no definition knew about, with the kind
    /// the file implies — the caller creates them.
    pub unknown_habits: Vec<(String, HabitKind)>,
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
    for habit in habits.iter() {
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

    // A column for every numeric habit that recorded something this month.
    // Screen time and sleep used to be the only two and were written in by
    // name; they are ordinary habits now and reach the table the same way any
    // other number does.
    let columns: Vec<&Habit> =
        habits.iter().filter(|h| logs.iter().any(|log| log.value(h.id).is_some())).collect();
    let mut measured: Vec<&DayLog> =
        logs.iter().filter(|log| columns.iter().any(|h| log.value(h.id).is_some())).collect();

    if !columns.is_empty() && !measured.is_empty() {
        measured.sort_by_key(|log| log.date);
        let names: Vec<&str> = columns.iter().map(|h| h.name.as_str()).collect();
        let rule = vec!["---"; columns.len() + 1];
        out.push_str(&format!("\n## Days\n\n| Day | {} |\n", names.join(" | ")));
        out.push_str(&format!("| {} |\n", rule.join(" | ")));

        for log in measured {
            let cells: Vec<String> = columns
                .iter()
                .map(|h| {
                    log.value(h.id)
                        .map(|v| {
                            if h.kind == HabitKind::Check {
                                v.to_string()
                            } else {
                                h.kind.format(v)
                            }
                        })
                        .unwrap_or_default()
                })
                .collect();
            out.push_str(&format!("| {} | {} |\n", log.date.day(), cells.join(" | ")));
        }
    }

    if !journal.entries.is_empty() {
        out.push_str("\n## Journal\n");
        for entry in &journal.entries {
            out.push_str(&format!(
                "\n### {} ^{}\n",
                entry.title.replace('\n', " ").trim(),
                entry.id
            ));
            if !entry.body.trim().is_empty() {
                for line in entry.body.lines() {
                    out.push_str("> ");
                    out.push_str(line);
                    out.push('\n');
                }
            }
        }
    }

    out
}

/// Reads one month file. `known` maps habit name (lowercased) to its id.
pub fn read_month(text: &str, year: i32, month: u32, known: &HashMap<String, HabitId>) -> Month {
    read_month_with_kinds(text, year, month, known, &HashMap::new())
}

pub fn read_month_with_kinds(
    text: &str,
    year: i32,
    month: u32,
    known: &HashMap<String, HabitId>,
    kinds: &HashMap<HabitId, HabitKind>,
) -> Month {
    let mut result = Month::default();
    let mut logs: HashMap<u32, DayLog> = HashMap::new();
    let mut section = Section::None;
    let mut entry: Option<JournalEntry> = None;
    let mut body = String::new();
    // Each Days column: the habit it names, and the kind once one value has
    // settled it.
    let mut columns: Vec<(String, Option<HabitKind>)> = Vec::new();

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
                let (title, id) = title
                    .rsplit_once(" ^")
                    .and_then(|(t, id)| Uuid::parse_str(id).ok().map(|id| (t, id)))
                    .unwrap_or_else(|| {
                        (
                            title.trim(),
                            Uuid::new_v5(
                                &Uuid::NAMESPACE_OID,
                                format!("journal:{year}:{month}:{}:{title}", result.journal.len())
                                    .as_bytes(),
                            ),
                        )
                    });
                let mut fresh = JournalEntry::new(title);
                fresh.id = id;
                entry = Some(fresh);
                continue;
            }
            if entry.is_some() {
                body.push_str(line.strip_prefix("> ").unwrap_or(line));
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
                let id = resolve(&mut result, known, name, HabitKind::Check);
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
                // The first row is the header: its cells after "Day" name the
                // habits the columns belong to. An older file whose header
                // reads "Day | Screen | Sleep" needs no special case — those
                // are two habit names like any other.
                let Ok(day) = row[0].parse::<u32>() else {
                    columns = row[1..].iter().map(|name| (name.trim().to_string(), None)).collect();
                    continue;
                };
                let Some(date) = day_of(day) else { continue };

                for (index, cell) in row[1..].iter().enumerate() {
                    let Some((name, resolved)) = columns.get_mut(index) else { continue };
                    if cell.trim().is_empty() {
                        continue;
                    }
                    // The kind is read off the first value the column shows:
                    // "7h 30m" is a length of time, "42" is a count. Guessing
                    // once is what lets a column be written by hand.
                    let kind = known
                        .get(&name.to_lowercase())
                        .and_then(|id| kinds.get(id))
                        .cloned()
                        .or_else(|| resolved.clone())
                        .unwrap_or_else(|| guess_kind(cell));
                    let id = resolve(&mut result, known, name, kind.clone());
                    *resolved = Some(kind.clone());
                    if let Some(value) = kind.parse_value(cell) {
                        logs.entry(day)
                            .or_insert_with(|| DayLog::new(date))
                            .set_value(id, value.into());
                    }
                }
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

/// Finds the habit a name refers to, noting it for creation if nothing knows
/// it yet. A habit invented by a month file needs a stable id immediately, so
/// the entries in that same file can point at it.
fn resolve(
    month: &mut Month,
    known: &HashMap<String, HabitId>,
    name: &str,
    kind: HabitKind,
) -> HabitId {
    if let Some(id) = known.get(&name.to_lowercase()) {
        return *id;
    }
    if !month.unknown_habits.iter().any(|(n, _)| n.eq_ignore_ascii_case(name)) {
        month.unknown_habits.push((name.to_string(), kind));
    }
    derived_habit_id(name)
}

/// What kind of number a cell holds, from how it is written.
fn guess_kind(cell: &str) -> HabitKind {
    let lowered = cell.to_lowercase();
    if (lowered.contains('h') || lowered.contains('m'))
        && crate::daily::parse_duration(cell).is_some()
    {
        HabitKind::Duration
    } else {
        HabitKind::Number { unit: String::new() }
    }
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

    fn numeric(name: &str, kind: HabitKind, position: i64) -> Habit {
        let mut habit = Habit::new(name, date(1), position);
        habit.kind = kind;
        habit
    }

    #[test]
    fn a_month_round_trips() {
        let gym = Habit::new("Gym", date(1), 0);
        let read = Habit::new("Read", date(1), 1);
        let sleep = numeric("Sleep", HabitKind::Duration, 2);
        let pages = numeric("Pages", HabitKind::Number { unit: "pages".into() }, 3);

        let mut logs = vec![DayLog::new(date(1)), DayLog::new(date(3)), DayLog::new(date(5))];
        logs[0].habits_done.push(gym.id);
        logs[0].set_value(sleep.id, Some(450.0));
        logs[0].set_value(pages.id, Some(42.0));
        logs[1].habits_done.push(gym.id);
        logs[1].habits_done.push(read.id);
        logs[2].set_value(sleep.id, Some(400.0));

        let mut journal = MonthJournal::new(2026, 9);
        let mut entry = JournalEntry::new("Week one");
        entry.body = "It went fine.\n\nMostly.".into();
        journal.entries.push(entry);

        let all = [gym.clone(), read.clone(), sleep.clone(), pages.clone()];
        let text = write_month(2026, 9, &all, &logs, &journal);

        let known: HashMap<String, HabitId> =
            all.iter().map(|h| (h.name.to_lowercase(), h.id)).collect();
        let back = read_month(&text, 2026, 9, &known);

        assert!(back.unknown_habits.is_empty(), "wrote:\n{text}");
        assert_eq!(back.logs.len(), 3);
        assert_eq!(back.logs[0].date, date(1));
        assert!(back.logs[0].is_done(gym.id));
        assert_eq!(back.logs[0].value(sleep.id), Some(450.0));
        assert_eq!(back.logs[0].value(pages.id), Some(42.0));
        assert!(back.logs[1].is_done(read.id));
        assert_eq!(back.logs[2].value(sleep.id), Some(400.0));
        assert_eq!(back.logs[2].value(pages.id), None);

        assert_eq!(back.journal.len(), 1);
        assert_eq!(back.journal[0].title, "Week one");
        assert_eq!(back.journal[0].body, "It went fine.\n\nMostly.");
    }

    /// The reason ticks name their habit instead of carrying an id.
    #[test]
    fn a_habit_named_only_in_a_month_file_is_reported_as_new() {
        let text = "# September 2026\n\n## Ticks\n\n- Swim: 5 6 7\n";
        let back = read_month(text, 2026, 9, &HashMap::new());
        assert_eq!(back.unknown_habits, [("Swim".to_string(), HabitKind::Check)]);
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

    /// A number column written by hand: the kind is read off the value, so a
    /// column can be added to a month file with nothing else set up.
    #[test]
    fn a_numeric_column_written_by_hand_creates_the_habit() {
        let text = "## Days\n\n| Day | Sleep | Pages |\n| --- | --- | --- |\n| 4 | 7h 30m | 42 |\n";
        let back = read_month(text, 2026, 9, &HashMap::new());

        assert_eq!(
            back.unknown_habits,
            [
                ("Sleep".to_string(), HabitKind::Duration),
                ("Pages".to_string(), HabitKind::Number { unit: String::new() }),
            ],
            "a time reads as a duration, a bare count as a number"
        );
        assert_eq!(back.logs.len(), 1);
        assert_eq!(back.logs[0].value(derived_habit_id("Sleep")), Some(450.0));
        assert_eq!(back.logs[0].value(derived_habit_id("Pages")), Some(42.0));
    }

    /// Vaults written before habits had kinds have a "Day | Screen | Sleep"
    /// table. It needs no special case — those are two habit names like any
    /// other — but it must keep reading, so this pins it.
    #[test]
    fn an_older_months_screen_and_sleep_table_still_reads() {
        let text = "# September 2026\n\n## Days\n\n| Day | Screen | Sleep |\n                    | --- | --- | --- |\n| 1 | 3h 20m | 7h 30m |\n| 2 |  | 8h |\n";
        let back = read_month(text, 2026, 9, &HashMap::new());

        let screen = derived_habit_id("Screen");
        let sleep = derived_habit_id("Sleep");
        assert_eq!(back.logs.len(), 2);
        assert_eq!(back.logs[0].value(screen), Some(200.0));
        assert_eq!(back.logs[0].value(sleep), Some(450.0));
        assert_eq!(back.logs[1].value(screen), None, "an empty cell is not a zero");
        assert_eq!(back.logs[1].value(sleep), Some(480.0));
    }

    #[test]
    fn habit_kinds_survive_their_definition_file() {
        let habits = vec![
            Habit::new("Gym", date(1), 0),
            numeric("Sleep", HabitKind::Duration, 1),
            numeric("Water", HabitKind::Number { unit: "glasses".into() }, 2),
        ];
        let back = read_habits(&write_habits(&habits), date(20));
        assert_eq!(back.len(), 3);
        assert_eq!(back[0].kind, HabitKind::Check);
        assert_eq!(back[1].kind, HabitKind::Duration);
        assert_eq!(back[2].kind, HabitKind::Number { unit: "glasses".into() });
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
        let sleep = numeric("Sleep", HabitKind::Duration, 1);
        let mut log = DayLog::new(date(2));
        log.habits_done.push(gym.id);
        log.set_value(sleep.id, Some(90.0));

        let habits = [gym.clone(), sleep.clone()];
        let journal = MonthJournal::new(2026, 9);
        let once = write_month(2026, 9, &habits, std::slice::from_ref(&log), &journal);
        let known: HashMap<String, HabitId> =
            habits.iter().map(|h| (h.name.to_lowercase(), h.id)).collect();
        let back = read_month(&once, 2026, 9, &known);
        let twice = write_month(2026, 9, &habits, &back.logs, &journal);
        assert_eq!(once, twice);
    }
}
