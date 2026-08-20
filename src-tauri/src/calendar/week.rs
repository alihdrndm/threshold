//! The coming week's shape, for the dashboard's free/busy panel.
//!
//! One command's worth of plumbing: a rolling seven-day window of opaque busy
//! intervals from Google plus the working hours, in a single payload. No
//! judgement lives here - which pixels those intervals become is the
//! frontend's pure `week.ts`, where it is tested - and nothing is stored:
//! only a short-lived in-memory cache, because free/busy cannot meaningfully
//! change faster than the poller that reconciles it (120 s), and a tab flip
//! should not cost a Google round-trip.

use std::sync::Mutex;
use std::time::Instant;

use chrono::{Datelike, Duration, Local, TimeZone};
use tauri::{AppHandle, Manager};

use super::{api::Client, settings, slot};
use crate::db::{self, Db};

/// One opaque busy interval, unix seconds. Google does not say what it is,
/// only that the time is taken - which is all a free/busy view needs.
#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct WeekBusy {
    pub start_ts: i64,
    pub end_ts: i64,
}

/// Working hours as the frontend needs them. `slot::WorkingHours` stays free
/// of serde on purpose - it is the scheduling algorithm's type - so this is a
/// plain copy at the boundary.
#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct WeekHours {
    pub start_min: u32,
    pub end_min: u32,
    /// Monday first, matching `slot::WorkingHours::days`.
    pub days: [bool; 7],
    pub buffer_min: u32,
}

impl From<slot::WorkingHours> for WeekHours {
    fn from(h: slot::WorkingHours) -> Self {
        Self {
            start_min: h.start_min,
            end_min: h.end_min,
            days: h.days,
            buffer_min: h.buffer_min,
        }
    }
}

/// Everything the week panel renders from, in one round-trip.
#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CalendarWeek {
    /// Local midnight today - the window's left edge, unix seconds.
    pub start_ts: i64,
    /// Local midnight seven days on. Stepped by date, not by 7 x 86 400
    /// seconds, so a DST week still ends at a true midnight.
    pub end_ts: i64,
    pub busy: Vec<WeekBusy>,
    pub hours: WeekHours,
    /// When Google was actually asked, so staleness is knowable.
    pub fetched_ts: i64,
}

/// The last successful fetch, managed on the app. Guarded by both age and
/// window: at local midnight the week rolls forward, and yesterday's cache -
/// however young - answers the wrong question.
#[derive(Default)]
pub struct WeekCache(pub Mutex<Option<(Instant, CalendarWeek)>>);

const TTL_SECS: u64 = 120;

/// Whether a cache entry may serve: young, and for the same day-window.
/// Pure so the two ways a cache can lie - age and midnight - are pinned by tests.
pub fn cache_fresh(age_secs: u64, entry_start_ts: i64, window_start_ts: i64) -> bool {
    age_secs < TTL_SECS && entry_start_ts == window_start_ts
}

/// Drop the cache. Called after any mutation that changes the calendar, so a
/// reschedule shows in the panel immediately rather than in up to two minutes.
pub fn invalidate(app: &AppHandle) {
    if let Some(cache) = app.try_state::<WeekCache>() {
        if let Ok(mut held) = cache.0.lock() {
            *held = None;
        }
    }
}

/// The coming week: cache if fresh, Google otherwise. Never holds the DB lock
/// across the HTTP call (the module rule in `sync`).
pub fn fetch(app: &AppHandle) -> Result<CalendarWeek, String> {
    let now = Local::now();
    let start = slot::day_start(now);
    let end_date = start.date_naive() + Duration::days(7);
    let end = Local
        .with_ymd_and_hms(end_date.year(), end_date.month(), end_date.day(), 0, 0, 0)
        .single()
        .ok_or("could not resolve the week's end")?;

    // Hours and connection state, in one scoped lock.
    let (hours, connected): (WeekHours, bool) = {
        let db = app.state::<Db>();
        let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
        (
            settings::read_hours(&conn).into(),
            db::get_setting(&conn, "google_connected").as_deref() == Some("1"),
        )
    };
    if !connected {
        return Err("Not connected - connect Google Calendar in Settings".into());
    }

    // A young cache for the same window serves, but always with today's hours:
    // they are local and cheap, and a settings change should show at once.
    {
        let cache = app.state::<WeekCache>();
        let held = cache.0.lock().map_err(|_| "cache lock poisoned")?;
        if let Some((at, week)) = held.as_ref() {
            if cache_fresh(at.elapsed().as_secs(), week.start_ts, start.timestamp()) {
                let mut week = week.clone();
                week.hours = hours;
                return Ok(week);
            }
        }
    }

    let client = Client::authed(app)?;
    let mut busy: Vec<WeekBusy> = client
        .free_busy(start, end)?
        .into_iter()
        .map(|(s, e)| WeekBusy {
            start_ts: s.timestamp(),
            end_ts: e.timestamp(),
        })
        .collect();
    busy.sort_by_key(|b| b.start_ts);

    let week = CalendarWeek {
        start_ts: start.timestamp(),
        end_ts: end.timestamp(),
        busy,
        hours,
        fetched_ts: Local::now().timestamp(),
    };

    {
        let cache = app.state::<WeekCache>();
        let mut held = cache.0.lock().map_err(|_| "cache lock poisoned")?;
        *held = Some((Instant::now(), week.clone()));
    }
    Ok(week)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_young_cache_for_the_same_window_serves() {
        assert!(cache_fresh(0, 1_000, 1_000));
        assert!(cache_fresh(TTL_SECS - 1, 1_000, 1_000));
    }

    #[test]
    fn an_aged_cache_does_not_serve() {
        assert!(!cache_fresh(TTL_SECS, 1_000, 1_000));
        assert!(!cache_fresh(TTL_SECS * 10, 1_000, 1_000));
    }

    #[test]
    fn a_cache_from_before_midnight_does_not_serve_however_young() {
        // The window rolled: same age, different start.
        assert!(!cache_fresh(5, 1_000, 1_000 + 86_400));
    }

    #[test]
    fn week_hours_carry_the_working_hours_defaults() {
        let hours: WeekHours = slot::WorkingHours::default().into();
        assert_eq!(hours.start_min, 540);
        assert_eq!(hours.end_min, 1080);
        assert_eq!(hours.days, [true, true, true, true, true, false, false]);
        assert_eq!(hours.buffer_min, 15);
    }
}
