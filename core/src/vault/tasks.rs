//! Tasks as Markdown checklists.
//!
//! One file per project, one line per task, in a shape Obsidian and every other
//! Markdown editor already renders:
//!
//! ```text
//! # Health
//!
//! - [ ] Gym due:2026-09-20 at:17:00 repeat:"every day" #fitness !p1 ^7f3a9c2e-…
//!   - [ ] warm up
//!   - [x] stretch
//!   > Bring the new shoes.
//! ```
//!
//! Two rules make the file safe to edit by hand:
//!
//! 1. **Metadata is a suffix.** Reading walks tokens from the end of the line
//!    and stops at the first one it does not recognize; everything before that
//!    is the title. So a `#` or a `due:` in the middle of a title stays in the
//!    title, which is what someone typing prose would expect.
//! 2. **A line with no `^id` is new**, so it goes through the ordinary
//!    natural-language parser. Jotting `- [ ] gym every day 5pm` into a file
//!    from any editor gives you the same task the capture bar would have.
//!    Existing lines are read strictly instead: the app wrote them, and
//!    re-parsing a title someone edited could silently move a date.

use chrono::{NaiveDate, NaiveTime};
use uuid::Uuid;

use crate::parse::parse_at;
use crate::task::{Checklist, Exception, Priority, Subtask, Task};

const DATE: &str = "%Y-%m-%d";
const TIME: &str = "%H:%M";

/// Namespace for deriving subtask ids from their titles, so a checklist item
/// needs no visible id in the file. Any v4 uuid would do; this one is fixed so
/// the derivation is stable across machines and runs.
const SUBTASK_NS: Uuid = Uuid::from_bytes([
    0x6d, 0x74, 0x6f, 0x64, 0x6f, 0x2d, 0x73, 0x75, 0x62, 0x74, 0x61, 0x73, 0x6b, 0x2d, 0x76, 0x31,
]);

/// Renders one project's tasks as a Markdown document.
pub fn write_file(project: Option<&str>, tasks: &[Task]) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", project.unwrap_or("Inbox")));
    for task in tasks {
        write_task(task, &mut out);
    }
    out
}

fn write_task(task: &Task, out: &mut String) {
    out.push_str(if task.is_done() { "- [x] " } else { "- [ ] " });
    out.push_str(&one_line(&task.title));

    if let Some(due) = task.due {
        out.push_str(&format!(" due:{}", due.format(DATE)));
    }
    if let Some(time) = task.time {
        out.push_str(&format!(" at:{}", time.format(TIME)));
    }
    if let Some(rule) = task.recurrence {
        out.push_str(&format!(" repeat:{}", quote(&rule.describe())));
    }
    for exception in &task.exceptions {
        match exception.action {
            crate::task::ExceptionAction::Skip => {
                out.push_str(&format!(" skip:{}", exception.date.format(DATE)));
            }
            crate::task::ExceptionAction::MoveTo(to) => {
                out.push_str(&format!(
                    " moved:{}>{}",
                    exception.date.format(DATE),
                    to.format(DATE)
                ));
            }
        }
    }
    if task.checklist == Checklist::OnePerOccurrence {
        out.push_str(" list:backlog");
    }
    for tag in &task.tags {
        out.push_str(&format!(" #{tag}"));
    }
    if let Some(flag) = priority_flag(task.priority) {
        out.push_str(&format!(" {flag}"));
    }
    if let Some(done) = task.completed_at {
        out.push_str(&format!(" done:{}", done.format(DATE)));
    }
    out.push_str(&format!(" created:{}", task.created_at.format(DATE)));
    out.push_str(&format!(" ^{}\n", task.id));

    for sub in &task.subtasks {
        out.push_str(if sub.done { "  - [x] " } else { "  - [ ] " });
        out.push_str(&one_line(&sub.title));
        out.push('\n');
    }
    for line in task.notes.lines() {
        out.push_str(if line.is_empty() { ">" } else { "> " });
        out.push_str(line);
        out.push('\n');
    }
    out.push('\n');
}

/// Reads a project file back into tasks.
///
/// `today` dates anything the file did not say, and anchors the
/// natural-language parse of hand-written lines.
pub fn read_file(text: &str, today: NaiveDate) -> (Option<String>, Vec<Task>) {
    let mut project = None;
    let mut tasks: Vec<Task> = Vec::new();

    for raw in text.lines() {
        let trimmed = raw.trim_start();
        let indented = raw.len() - trimmed.len() > 0;

        if let Some(heading) = trimmed.strip_prefix("# ") {
            if project.is_none() {
                let name = heading.trim();
                project = (!name.eq_ignore_ascii_case("inbox")).then(|| name.to_string());
            }
            continue;
        }

        if let Some(body) = checkbox(trimmed) {
            if indented {
                // An indented checkbox belongs to the task above it.
                if let Some(task) = tasks.last_mut() {
                    let (done, title) = body;
                    let title = strip_id(title).0.trim().to_string();
                    if !title.is_empty() {
                        task.subtasks.push(Subtask {
                            id: subtask_id(task.id, &title, task.subtasks.len()),
                            title,
                            done,
                        });
                    }
                    continue;
                }
            }
            let (done, line) = body;
            if let Some(task) = read_task(line, done, today) {
                tasks.push(task);
            }
            continue;
        }

        if let Some(note) = trimmed.strip_prefix('>') {
            if let Some(task) = tasks.last_mut() {
                if !task.notes.is_empty() {
                    task.notes.push('\n');
                }
                task.notes.push_str(note.strip_prefix(' ').unwrap_or(note));
            }
        }
    }

    (project, tasks)
}

/// `- [ ] rest` / `- [x] rest`, also accepting `*` as the bullet.
fn checkbox(line: &str) -> Option<(bool, &str)> {
    let rest = line.strip_prefix("- ").or_else(|| line.strip_prefix("* "))?;
    let rest = rest.trim_start();
    for (marker, done) in [("[ ]", false), ("[x]", true), ("[X]", true)] {
        if let Some(body) = rest.strip_prefix(marker) {
            return Some((done, body.trim_start()));
        }
    }
    None
}

fn read_task(line: &str, done: bool, today: NaiveDate) -> Option<Task> {
    let tokens = tokenize(line);
    // Walk back from the end while tokens are recognizable metadata, never
    // consuming the whole line — a task whose title *is* "#fitness" should keep
    // it, not end up blank.
    let mut first_meta = tokens.len();
    while first_meta > 1 && is_meta(&tokens[first_meta - 1]) {
        first_meta -= 1;
    }
    let title = tokens[..first_meta].join(" ");
    let meta = &tokens[first_meta..];

    let has_id = meta.iter().any(|t| t.starts_with('^'));

    // A hand-written line is whatever the capture bar would have made of it;
    // explicit fields then override what the parser guessed.
    let mut task = if has_id {
        let mut task = Task::new(title.clone(), today);
        task.title = title;
        task
    } else {
        let parsed = parse_at(&title, today.and_hms_opt(9, 0, 0)?);
        parsed.task
    };
    task.created_at = today;
    if done {
        task.completed_at = Some(today);
    }

    for token in meta {
        apply(&mut task, token);
    }

    // A completed one-off must carry a date; a file that says `[x]` with no
    // `done:` still means done.
    if done && task.completed_at.is_none() {
        task.completed_at = Some(today);
    }
    if !done {
        task.completed_at = None;
    }

    (!task.title.trim().is_empty()).then_some(task)
}

/// Whether a token is a metadata field rather than part of the title.
fn is_meta(token: &str) -> bool {
    if token.starts_with('^') || token.starts_with('#') {
        return token.len() > 1;
    }
    if priority_of(token).is_some() {
        return true;
    }
    matches!(
        token.split_once(':').map(|(key, _)| key),
        Some("due" | "at" | "repeat" | "every" | "skip" | "moved" | "done" | "created" | "list")
    )
}

fn apply(task: &mut Task, token: &str) {
    if let Some(id) = token.strip_prefix('^') {
        if let Ok(id) = Uuid::parse_str(id) {
            task.id = id;
        }
        return;
    }
    if let Some(tag) = token.strip_prefix('#') {
        task.add_tag(tag);
        return;
    }
    if let Some(priority) = priority_of(token) {
        task.priority = priority;
        return;
    }
    let Some((key, value)) = token.split_once(':') else { return };
    let value = unquote(value);
    match key {
        "due" => task.due = date(&value),
        "at" => task.time = time(&value),
        // "every" is accepted as a synonym because it is what someone writing
        // the line by hand reaches for.
        "repeat" | "every" => {
            task.recurrence = crate::parse::recurrence_from_phrase(&value)
                .or_else(|| crate::parse::recurrence_from_phrase(&format!("every {value}")))
        }
        "skip" => {
            if let Some(date) = date(&value) {
                task.exceptions.push(Exception::skip(date));
            }
        }
        "moved" => {
            if let Some((from, to)) = value.split_once('>') {
                if let (Some(from), Some(to)) = (date(from), date(to)) {
                    task.exceptions.push(Exception::move_to(from, to));
                }
            }
        }
        "done" => task.completed_at = date(&value),
        "created" => {
            if let Some(date) = date(&value) {
                task.created_at = date;
            }
        }
        "list" => {
            task.checklist = match value.as_str() {
                "backlog" | "one-per-occurrence" => Checklist::OnePerOccurrence,
                _ => Checklist::All,
            }
        }
        _ => {}
    }
}

fn date(text: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(text.trim(), DATE).ok()
}

fn time(text: &str) -> Option<NaiveTime> {
    let text = text.trim();
    NaiveTime::parse_from_str(text, TIME)
        .or_else(|_| NaiveTime::parse_from_str(text, "%H:%M:%S"))
        .ok()
}

fn priority_flag(priority: Priority) -> Option<&'static str> {
    match priority {
        Priority::High => Some("!p1"),
        Priority::Medium => Some("!p2"),
        Priority::Low => Some("!p3"),
        Priority::None => None,
    }
}

fn priority_of(token: &str) -> Option<Priority> {
    let rest = token.strip_prefix('!')?;
    Priority::parse(rest).filter(|p| *p != Priority::None)
}

/// Splits on whitespace, keeping a double-quoted run together — which is how a
/// multi-word `repeat:"every other friday"` survives.
fn tokenize(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for ch in line.chars() {
        match ch {
            '"' => {
                quoted = !quoted;
                current.push(ch);
            }
            c if c.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn quote(value: &str) -> String {
    if value.contains(char::is_whitespace) {
        format!("\"{}\"", value.replace('"', "'"))
    } else {
        value.to_string()
    }
}

fn unquote(value: &str) -> String {
    value.trim().trim_matches('"').to_string()
}

fn strip_id(line: &str) -> (&str, Option<Uuid>) {
    match line.rsplit_once(" ^") {
        Some((head, id)) => match Uuid::parse_str(id.trim()) {
            Ok(id) => (head, Some(id)),
            Err(_) => (line, None),
        },
        None => (line, None),
    }
}

/// A checklist item's identity is its text, so the file needs no id for it.
/// The index disambiguates a list that repeats a title.
fn subtask_id(task: crate::task::TaskId, title: &str, index: usize) -> Uuid {
    let ns = Uuid::new_v5(&SUBTASK_NS, task.as_bytes());
    Uuid::new_v5(&ns, format!("{index}:{title}").as_bytes())
}

/// Titles are one line by construction; a pasted newline would otherwise split
/// a task in two on the next read.
fn one_line(text: &str) -> String {
    text.replace(['\n', '\r'], " ").trim().to_string()
}

/// A filename-safe form of a project name. Reading takes the display name from
/// the file's heading, so this only has to be stable and legible.
pub fn slug(project: Option<&str>) -> String {
    let Some(project) = project else { return "inbox".to_string() };
    let mut slug = String::new();
    for ch in project.trim().to_lowercase().chars() {
        if ch.is_alphanumeric() {
            slug.push(ch);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "project".to_string()
    } else {
        slug
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{Recurrence, WeekdaySet};
    use chrono::Weekday;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, 20).unwrap()
    }

    fn roundtrip(task: &Task) -> Task {
        let text = write_file(Some("Health"), std::slice::from_ref(task));
        let (_, mut tasks) = read_file(&text, today());
        assert_eq!(tasks.len(), 1, "wrote:\n{text}");
        tasks.remove(0)
    }

    #[test]
    fn a_full_task_survives_the_round_trip() {
        let mut task = Task::new("Gym", NaiveDate::from_ymd_opt(2026, 9, 13).unwrap());
        task.due = NaiveDate::from_ymd_opt(2026, 9, 20);
        task.time = NaiveTime::from_hms_opt(17, 0, 0);
        task.recurrence =
            Some(Recurrence::EveryNWeeks { n: 2, days: WeekdaySet::from_day(Weekday::Fri) });
        task.priority = Priority::High;
        task.add_tag("fitness");
        task.add_tag("health");
        task.notes = "Bring the new shoes.\n\nAnd a towel.".into();
        task.checklist = Checklist::OnePerOccurrence;
        task.subtasks = vec![Subtask::new("warm up"), Subtask::new("stretch")];
        task.subtasks[1].done = true;
        task.exceptions = vec![Exception::skip(NaiveDate::from_ymd_opt(2026, 10, 2).unwrap())];

        let back = roundtrip(&task);
        assert_eq!(back.id, task.id);
        assert_eq!(back.title, "Gym");
        assert_eq!(back.due, task.due);
        assert_eq!(back.time, task.time);
        assert_eq!(back.recurrence, task.recurrence);
        assert_eq!(back.priority, Priority::High);
        assert_eq!(back.tags, ["fitness", "health"]);
        assert_eq!(back.notes, task.notes);
        assert_eq!(back.checklist, Checklist::OnePerOccurrence);
        assert_eq!(back.created_at, task.created_at);
        assert_eq!(back.exceptions, task.exceptions);
        assert_eq!(back.subtasks.len(), 2);
        assert_eq!(back.subtasks[0].title, "warm up");
        assert!(!back.subtasks[0].done);
        assert!(back.subtasks[1].done);
    }

    #[test]
    fn subtask_ids_are_stable_across_reads() {
        let mut task = Task::new("Learning", today());
        task.subtasks = vec![Subtask::new("rust"), Subtask::new("cooking")];
        let first = roundtrip(&task);
        let second = roundtrip(&task);
        assert_eq!(first.subtasks[0].id, second.subtasks[0].id);
        assert_eq!(first.subtasks[1].id, second.subtasks[1].id);
        assert_ne!(first.subtasks[0].id, first.subtasks[1].id);
    }

    #[test]
    fn a_completed_task_reads_back_completed() {
        let mut task = Task::new("Ship it", today());
        task.completed_at = NaiveDate::from_ymd_opt(2026, 9, 19);
        let back = roundtrip(&task);
        assert_eq!(back.completed_at, task.completed_at);
    }

    /// The point of the whole format: type a line into the file from any
    /// editor and get the task the capture bar would have made.
    #[test]
    fn a_hand_written_line_is_parsed_as_natural_language() {
        let (_, tasks) = read_file("# Inbox\n\n- [ ] gym every day 5pm #health !p1\n", today());
        assert_eq!(tasks.len(), 1);
        let task = &tasks[0];
        assert_eq!(task.title, "gym");
        assert_eq!(task.recurrence, Some(Recurrence::Daily));
        assert_eq!(task.time, NaiveTime::from_hms_opt(17, 0, 0));
        assert_eq!(task.tags, ["health"]);
        assert_eq!(task.priority, Priority::High);
    }

    #[test]
    fn a_hand_written_line_may_also_use_explicit_fields() {
        let (_, tasks) =
            read_file("- [ ] Dentist due:2026-10-01 at:09:30 repeat:\"every 3 months\"\n", today());
        let task = &tasks[0];
        assert_eq!(task.title, "Dentist");
        assert_eq!(task.due, NaiveDate::from_ymd_opt(2026, 10, 1));
        assert_eq!(task.time, NaiveTime::from_hms_opt(9, 30, 0));
        assert_eq!(task.recurrence, Some(Recurrence::EveryNMonths(3)));
    }

    /// Someone writing by hand reaches for `every:` before `repeat:`, and a
    /// bare "day" before "every day".
    #[test]
    fn every_is_accepted_as_a_synonym() {
        let (_, tasks) = read_file("- [ ] Walk every:day\n", today());
        assert_eq!(tasks[0].recurrence, Some(Recurrence::Daily));
    }

    #[test]
    fn an_existing_line_is_not_re_parsed() {
        // The title says "tomorrow" but the line has an id, so it was written
        // by the app: the word stays in the title and the date stands.
        let id = Uuid::new_v4();
        let text = format!("- [ ] call mom tomorrow due:2026-12-01 created:2026-09-20 ^{id}\n");
        let (_, tasks) = read_file(&text, today());
        assert_eq!(tasks[0].title, "call mom tomorrow");
        assert_eq!(tasks[0].due, NaiveDate::from_ymd_opt(2026, 12, 1));
    }

    #[test]
    fn metadata_in_the_middle_of_a_title_stays_in_the_title() {
        let id = Uuid::new_v4();
        let text = format!("- [ ] read #5 of the series !p1 ^{id}\n");
        let (_, tasks) = read_file(&text, today());
        assert_eq!(tasks[0].title, "read #5 of the series");
        assert_eq!(tasks[0].priority, Priority::High);
    }

    #[test]
    fn a_title_that_looks_like_metadata_is_kept() {
        let mut task = Task::new("#fitness", today());
        task.add_tag("fitness");
        let back = roundtrip(&task);
        assert_eq!(back.title, "#fitness");
        assert_eq!(back.tags, ["fitness"]);
    }

    #[test]
    fn the_heading_names_the_project_and_inbox_means_none() {
        let (project, _) = read_file("# Home Chores\n\n- [ ] sweep\n", today());
        assert_eq!(project.as_deref(), Some("Home Chores"));
        let (project, _) = read_file("# Inbox\n\n- [ ] sweep\n", today());
        assert_eq!(project, None);
    }

    #[test]
    fn slugs_are_filename_safe() {
        assert_eq!(slug(None), "inbox");
        assert_eq!(slug(Some("Health")), "health");
        assert_eq!(slug(Some("Home / Chores")), "home-chores");
        assert_eq!(slug(Some("!!!")), "project");
    }

    #[test]
    fn a_file_with_junk_in_it_still_reads() {
        let text = "# Inbox\n\nrandom prose nobody asked about\n\n- [ ] real task\n\n---\n";
        let (_, tasks) = read_file(text, today());
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "real task");
    }

    #[test]
    fn writing_is_stable_so_an_unchanged_vault_produces_no_diff() {
        let mut task = Task::new("Gym", today());
        task.due = Some(today());
        task.add_tag("health");
        let once = write_file(Some("Health"), &[task.clone()]);
        let (_, tasks) = read_file(&once, today());
        let twice = write_file(Some("Health"), &tasks);
        assert_eq!(once, twice);
    }
}
