//! Reminders, and the tray tooltip.
//!
//! Spec §4 asks for two things this module covers: notifications when a task
//! comes due, and a tray icon you can hover to see what is waiting.
//!
//! The loop is deliberately dumb — it wakes on a timer and asks the store what
//! is due — rather than scheduling a timer per task. Tasks move, recurrence
//! shifts, the machine sleeps; a poll survives all of that, where a pile of
//! scheduled timers quietly goes stale.

use std::collections::HashSet;
use std::sync::Mutex;
use std::time::Duration;

use chrono::{Local, NaiveDate, NaiveTime, Timelike};
use mtodo_core::{recur, Filter, Sort, TaskId};
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::state::AppState;

/// How often to look for tasks coming due.
const TICK: Duration = Duration::from_secs(30);

/// Only notify for a time that passed within this window. Bounds what a restart
/// can replay: reopening the app at 6pm should not fire every reminder from
/// earlier in the day.
const GRACE_MINUTES: i64 = 5;

/// Which (task, day) pairs have already been announced this session.
#[derive(Default)]
pub struct Announced(Mutex<HashSet<(TaskId, NaiveDate)>>);

/// Starts the reminder loop and seeds the tray tooltip.
pub fn start(app: &AppHandle) {
    app.manage(Announced::default());
    update_tooltip(app);

    let handle = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(TICK);
        tick(&handle);
    });
}

fn tick(app: &AppHandle) {
    update_tooltip(app);

    let now = Local::now().naive_local();
    for (id, title, time) in due_now(app, now.date(), now.time()) {
        if claim(app, id, now.date()) {
            notify(app, &title, time);
        }
    }
}

/// Tasks due today whose time has just passed.
fn due_now(app: &AppHandle, today: NaiveDate, now: NaiveTime) -> Vec<(TaskId, String, NaiveTime)> {
    let Some(state) = app.try_state::<AppState>() else { return Vec::new() };
    let Ok(store) = state.store.lock() else { return Vec::new() };
    let Ok(tasks) = store.query(&Filter::Today, Sort::Due, today) else { return Vec::new() };

    tasks
        .into_iter()
        .filter_map(|task| {
            let time = task.time?;
            // The Today view also carries overdue work, which must not ring
            // again each day as the clock passes its old time. Only an
            // occurrence falling today is a reminder.
            if recur::occurrences(&task, today, today).is_empty() {
                return None;
            }
            let elapsed = (now - time).num_minutes();
            // Due in the recent past, not the future and not hours ago.
            (0..=GRACE_MINUTES).contains(&elapsed).then_some((task.id, task.title, time))
        })
        .collect()
}

/// Records that this task was announced today, returning false if it already
/// had been — so a task due at 5pm is not re-announced every 30 seconds.
fn claim(app: &AppHandle, id: TaskId, day: NaiveDate) -> bool {
    let Some(announced) = app.try_state::<Announced>() else { return false };
    let Ok(mut seen) = announced.0.lock() else { return false };
    seen.insert((id, day))
}

fn notify(app: &AppHandle, title: &str, time: NaiveTime) {
    let body = format!("Due at {}", format_hour(time));
    // A reminder that fails silently is worse than no reminder, because the
    // user trusts it and does not find out. Say so on stderr at least.
    if let Err(error) = app.notification().builder().title(title).body(body).show() {
        eprintln!("reminder for {title:?} could not be shown: {error}");
    }
}

/// "5:00 PM" without pulling in a formatting dependency.
fn format_hour(time: NaiveTime) -> String {
    let (pm, hour) = time.hour12();
    format!("{hour}:{:02} {}", time.minute(), if pm { "PM" } else { "AM" })
}

/// Puts the day's headline on the tray icon, so hovering it previews what is
/// waiting without opening anything.
pub fn update_tooltip(app: &AppHandle) {
    let Some(tray) = app.tray_by_id("tray") else { return };
    let text = summary(app).unwrap_or_else(|| "Master Todo".to_string());
    let _ = tray.set_tooltip(Some(format!("Master Todo — {text}")));
}

fn summary(app: &AppHandle) -> Option<String> {
    let state = app.try_state::<AppState>()?;
    let store = state.store.lock().ok()?;
    let today = Local::now().date_naive();
    let tasks = store.query(&Filter::Today, Sort::Due, today).ok()?;

    let overdue = tasks.iter().filter(|t| t.is_overdue(today)).count();
    // The first few titles, so a hover is actually a preview and not a count.
    let preview: Vec<&str> = tasks.iter().take(3).map(|t| t.title.as_str()).collect();

    let headline = match (tasks.len(), overdue) {
        (0, _) => return Some("nothing due today".into()),
        (n, 0) => format!("{n} due today"),
        (n, o) => format!("{n} due today, {o} overdue"),
    };
    let more = tasks.len().saturating_sub(preview.len());
    let listed = preview.join(", ");
    Some(if more > 0 {
        format!("{headline}\n{listed}, +{more} more")
    } else {
        format!("{headline}\n{listed}")
    })
}
