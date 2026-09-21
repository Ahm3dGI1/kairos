//! The prayer page.
//!
//! Nothing here calls anything. The times come from `kairos-core`, which works
//! them out from the date and the coordinates in settings — so the page is as
//! offline as the rest of the app, and stays right on a plane.

use chrono::{Datelike, Local, NaiveDate, Timelike};
use kairos_core::prayer;
use serde::Serialize;
use tauri::State;

use crate::commands::CmdResult;
use crate::state::AppState;

#[derive(Serialize)]
pub struct Prayer {
    name: &'static str,
    /// "5:12 AM", or empty when the sun never reaches the angle.
    time: String,
    /// Minutes since midnight, for the page's own arithmetic.
    minutes: Option<u32>,
    /// Sunrise is shown but is not a prayer.
    is_prayer: bool,
    is_next: bool,
    is_past: bool,
}

#[derive(Serialize)]
pub struct PrayerDay {
    date: NaiveDate,
    /// "Monday 21 September"
    label: String,
    is_today: bool,
    prayers: Vec<Prayer>,
    /// Absent until coordinates are set, which is what the page explains.
    located: bool,
    /// "Muslim World League · Hanafi", for the status line.
    method: String,
    /// Minutes until the next prayer, when the day on screen is today.
    until_next: Option<i64>,
    /// Set when the method could not place Fajr or Isha — a northern summer,
    /// where the sun never gets far enough below the horizon.
    no_angle: bool,
}

#[tauri::command]
pub fn prayer_day(state: State<'_, AppState>, date: Option<NaiveDate>) -> CmdResult<PrayerDay> {
    let settings = state.settings.lock().map_err(|_| "settings lock poisoned")?.clone();
    let now = Local::now();
    let today = now.date_naive();
    let date = date.unwrap_or(today);

    let location = settings.location();
    let params = settings.prayer_params();
    // Whatever Windows says the clock is offset by, in hours.
    let utc_offset = f64::from(now.offset().local_minus_utc()) / 3600.0;

    let times = prayer::times(date, location, params, utc_offset);
    let is_today = date == today;
    let current = now.time();
    let next = is_today.then(|| times.next_after(current)).flatten();

    let prayers = times
        .in_order()
        .into_iter()
        .enumerate()
        .map(|(index, time)| {
            let minutes = time.map(|t| t.hour() * 60 + t.minute());
            Prayer {
                name: prayer::NAMES[index],
                time: time.map(prayer::format).unwrap_or_default(),
                minutes,
                is_prayer: index != 1,
                is_next: next == Some(index),
                is_past: is_today && time.is_some_and(|t| t < current) && next != Some(index),
            }
        })
        .collect();

    let until_next =
        next.and_then(|index| times.in_order()[index]).map(|t| (t - current).num_minutes());

    Ok(PrayerDay {
        label: format!("{} {} {}", date.format("%A"), date.day(), date.format("%B"),),
        date,
        is_today,
        prayers,
        located: location.is_set(),
        method: format!(
            "{} · {}",
            params.method.label(),
            if params.asr == prayer::Asr::Hanafi { "Hanafi" } else { "Standard" }
        ),
        until_next,
        no_angle: location.is_set() && (times.fajr.is_none() || times.isha.is_none()),
    })
}
