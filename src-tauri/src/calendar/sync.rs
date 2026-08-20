//! The orchestration: task ↔ event, and the poll that reconciles the two.
//!
//! Every function here runs off the main thread and off the DB lock while it
//! talks to Google. The pattern is always the same: lock, read what we need,
//! unlock, do the HTTP, lock, write the result, unlock, then tell the UI with
//! an event. A task move never waits on any of this and never fails because of
//! it - a calendar error lands in a settings string and a `calendar-error`
//! event, and the task sits in Schedule with no date until the next try.

use std::sync::atomic::{AtomicBool, Ordering};

use chrono::{Duration, Local, TimeZone};
use tauri::{AppHandle, Emitter, Manager};

use super::{api::Client, settings, slot};
use crate::db::{self, tasks, Db};

/// 30 minutes, the fixed slot length.
const SLOT: Duration = Duration::minutes(30);

/// Set by a wake or a "sync now" so the poller runs its next loop immediately.
static POKE: AtomicBool = AtomicBool::new(false);

fn connected(app: &AppHandle) -> bool {
    let db = app.state::<Db>();
    db.0
        .lock()
        .ok()
        .and_then(|conn| db::get_setting(&conn, "google_connected"))
        .as_deref()
        == Some("1")
}

fn set_status(app: &AppHandle, status: &str) {
    let db = app.state::<Db>();
    let Ok(conn) = db.0.lock() else { return };
    let _ = db::set_setting(&conn, "google_last_sync_status", status);
    let _ = db::set_setting(
        &conn,
        "google_last_sync_ts",
        &chrono::Utc::now().timestamp().to_string(),
    );
}

/// Report a calendar failure without ever failing the task move that triggered it.
fn report(app: &AppHandle, err: &str) {
    crate::log::line(&format!("calendar: {err}"));
    set_status(app, err);
    let _ = app.emit("calendar-error", err);
}

/// A task just entered Schedule: find a slot and create its event. No-op when
/// not connected - the card simply shows "no date yet".
pub fn on_task_entered_schedule(app: AppHandle, task_id: i64) {
    if !connected(&app) {
        return;
    }
    std::thread::spawn(move || {
        if let Err(err) = schedule_now(&app, task_id) {
            report(&app, &err);
        }
    });
}

/// Create (or replace) the event for a task at the next free slot.
fn schedule_now(app: &AppHandle, task_id: i64) -> Result<(), String> {
    let (title, hours, old_event) = {
        let db = app.state::<Db>();
        let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
        // The task may have left Schedule again before this thread ran.
        if !tasks::is_in_schedule(&conn, task_id)? {
            return Ok(());
        }
        let task = tasks::by_id(&conn, task_id)?.ok_or("task vanished")?;
        (
            task.title,
            settings::read_hours(&conn),
            task.calendar_event_id,
        )
    };

    let client = Client::authed(app)?;
    // Replacing a stale link (e.g. after an undo) rather than orphaning it.
    if let Some(id) = old_event.as_deref() {
        let _ = client.delete_event(id);
    }

    let now = Local::now();
    let busy = client.free_busy(now, now + Duration::days(14))?;
    let start = slot::next_free_slot(now, &hours, SLOT, &busy)
        .ok_or("no free slot in the next two weeks - widen your working hours")?;
    create_event_at(app, &client, task_id, &title, start)
}

/// Insert a task's event at a fixed start and record it. The tail every
/// event-creating path shares: the slot hunt above, and the repeat advance
/// when the task had no event to move.
fn create_event_at(
    app: &AppHandle,
    client: &Client,
    task_id: i64,
    title: &str,
    start: chrono::DateTime<Local>,
) -> Result<(), String> {
    let event = client.insert_event(task_id, title, start, start + SLOT)?;
    {
        let db = app.state::<Db>();
        let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
        tasks::set_schedule(
            &conn,
            task_id,
            Some(start.timestamp()),
            Some(&event.id),
            event.html_link.as_deref(),
        )?;
    }
    set_status(app, "Synced");
    super::week::invalidate(app);
    let _ = app.emit("tasks-changed", ());
    Ok(())
}

/// A repeating task was completed: it does not leave the board, it moves.
///
/// The local advance lands first - the card must answer the click even with
/// no network - then the calendar follows on its own thread, reporting
/// failure the way every calendar error does. If the patch fails, the poll
/// may briefly show Google's old time until a retry lands; the same class of
/// drift as any offline edit, and the card's local time wins the next patch.
pub fn advance_repeating(app: AppHandle, task_id: i64, next_ts: i64) {
    let (event_id, title) = {
        let db = app.state::<Db>();
        let Ok(conn) = db.0.lock() else { return };
        let Some(task) = tasks::by_id(&conn, task_id).ok().flatten() else {
            return;
        };
        // None link = keep the stored one (set_schedule COALESCEs).
        let _ = tasks::set_schedule(
            &conn,
            task_id,
            Some(next_ts),
            task.calendar_event_id.as_deref(),
            None,
        );
        (task.calendar_event_id, task.title)
    };
    super::week::invalidate(&app);
    let _ = app.emit("tasks-changed", ());
    if !connected(&app) {
        return;
    }
    std::thread::spawn(move || {
        let go = || -> Result<(), String> {
            let client = Client::authed(&app)?;
            let start = Local
                .timestamp_opt(next_ts, 0)
                .single()
                .ok_or("that is not a valid time")?;
            match event_id.as_deref() {
                Some(id) => {
                    client.patch_event(id, start, start + SLOT)?;
                    set_status(&app, "Synced");
                    super::week::invalidate(&app);
                    let _ = app.emit("tasks-changed", ());
                    Ok(())
                }
                None => create_event_at(&app, &client, task_id, &title, start),
            }
        };
        if let Err(err) = go() {
            report(&app, &err);
        }
    });
}

/// A task left Schedule (moved, completed, deleted): delete its event.
pub fn on_task_left_schedule(app: AppHandle, task_id: i64) {
    let event = {
        let db = app.state::<Db>();
        let Ok(conn) = db.0.lock() else { return };
        let event = tasks::by_id(&conn, task_id)
            .ok()
            .flatten()
            .and_then(|t| t.calendar_event_id);
        let _ = tasks::clear_schedule(&conn, task_id);
        event
    };
    let Some(event_id) = event else { return };
    if !connected(&app) {
        return;
    }
    std::thread::spawn(move || match Client::authed(&app) {
        Ok(client) => {
            if let Err(err) = client.delete_event(&event_id) {
                report(&app, &err);
            } else {
                super::week::invalidate(&app);
                let _ = app.emit("tasks-changed", ());
            }
        }
        Err(err) => report(&app, &err),
    });
}

/// Move a scheduled task's event: `None` = the next free slot, `Some(ts)` = the
/// time the user picked. Runs inline (a command awaits it) but off the DB lock.
pub fn reschedule(app: &AppHandle, task_id: i64, start: Option<i64>) -> Result<i64, String> {
    let (event_id, hours, current) = {
        let db = app.state::<Db>();
        let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
        let task = tasks::by_id(&conn, task_id)?.ok_or("task vanished")?;
        (
            task.calendar_event_id,
            settings::read_hours(&conn),
            task.scheduled_ts,
        )
    };

    // No event yet (was offline when it landed): make one instead of moving one.
    let Some(event_id) = event_id else {
        schedule_now(app, task_id)?;
        let db = app.state::<Db>();
        let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
        return tasks::by_id(&conn, task_id)?
            .and_then(|t| t.scheduled_ts)
            .ok_or_else(|| "could not schedule".into());
    };

    let client = Client::authed(app)?;
    let start_local = match start {
        Some(ts) => Local
            .timestamp_opt(ts, 0)
            .single()
            .ok_or("that is not a valid time")?,
        None => {
            let now = Local::now();
            let mut busy = client.free_busy(now, now + Duration::days(14))?;
            // Ignore the task's own current block, so "next free slot" does not
            // treat where it already sits as an obstacle.
            if let Some(cur) = current {
                if let Some(cur_local) = Local.timestamp_opt(cur, 0).single() {
                    busy.retain(|(s, _)| *s != cur_local);
                }
            }
            slot::next_free_slot(now, &hours, SLOT, &busy)
                .ok_or("no free slot in the next two weeks")?
        }
    };

    client.patch_event(&event_id, start_local, start_local + SLOT)?;
    {
        let db = app.state::<Db>();
        let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
        tasks::set_schedule(
            &conn,
            task_id,
            Some(start_local.timestamp()),
            Some(&event_id),
            None,
        )?;
        // patch does not return htmlLink; keep the existing one.
    }
    super::week::invalidate(app);
    let _ = app.emit("tasks-changed", ());
    Ok(start_local.timestamp())
}

/// Take a scheduled task off the calendar but leave it in Schedule (dateless).
pub fn remove_from_calendar(app: &AppHandle, task_id: i64) -> Result<(), String> {
    let event = {
        let db = app.state::<Db>();
        let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
        let event = tasks::by_id(&conn, task_id)?.and_then(|t| t.calendar_event_id);
        tasks::clear_schedule(&conn, task_id)?;
        event
    };
    if let Some(id) = event {
        if connected(app) {
            let client = Client::authed(app)?;
            client.delete_event(&id)?;
            super::week::invalidate(app);
        }
    }
    let _ = app.emit("tasks-changed", ());
    Ok(())
}

/// One reconciliation pass: pull the app's events from Google and apply what
/// changed. Moved → update the slot; cancelled → clear the link (card shows
/// "no date", never silently re-created); an event whose task is no longer
/// scheduled → delete it. Returns a short status string.
pub fn poll_once(app: &AppHandle) -> Result<String, String> {
    if !connected(app) {
        return Ok("Not connected".into());
    }
    let client = Client::authed(app)?;
    let token = super::api::sync_token(app);
    let (items, next) = match client.list_task_events(token.as_deref()) {
        Ok(pair) => pair,
        Err(e) if e == "SYNC_TOKEN_EXPIRED" => {
            super::api::save_sync_token(app, None);
            client.list_task_events(None)?
        }
        Err(e) => return Err(e),
    };

    let mut changed = false;
    for item in &items {
        let Some(task_id) = item.task_id else { continue };
        let db = app.state::<Db>();
        let (linked, in_schedule, open) = {
            let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
            match tasks::by_id(&conn, task_id)? {
                Some(t) => (
                    t.calendar_event_id.as_deref() == Some(item.id.as_str()),
                    t.urgent == Some(false) && t.important == Some(true),
                    t.status == "open",
                ),
                None => (false, false, false),
            }
        };
        if !linked {
            continue;
        }
        if item.cancelled {
            let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
            tasks::clear_schedule(&conn, task_id)?;
            changed = true;
            continue;
        }
        // The user moved a still-scheduled task's event in Google.
        if in_schedule && open {
            if let Some(ts) = item.start_ts {
                let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
                let current = tasks::by_id(&conn, task_id)?.and_then(|t| t.scheduled_ts);
                if current != Some(ts) {
                    tasks::set_schedule(&conn, task_id, Some(ts), Some(&item.id), None)?;
                    changed = true;
                }
            }
        } else {
            // The task left Schedule while the event lingers: clean it up.
            drop(client.delete_event(&item.id));
            let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
            tasks::clear_schedule(&conn, task_id)?;
            changed = true;
        }
    }

    super::api::save_sync_token(app, next.as_deref());
    set_status(app, "Synced");
    if changed {
        super::week::invalidate(app);
        let _ = app.emit("tasks-changed", ());
    }
    Ok("Synced".into())
}

/// Ask the poller to run its next pass now (from a wake, or "Sync now").
pub fn request_poll() {
    POKE.store(true, Ordering::Relaxed);
}

/// The background reconciler. Long, quiet intervals; wakes early on a poke.
pub fn spawn_poller(app: AppHandle) {
    std::thread::Builder::new()
        .name("threshold-calendar".into())
        .spawn(move || {
            // Let startup settle before the first pass.
            std::thread::sleep(std::time::Duration::from_secs(20));
            let mut backoff = 0u64;
            loop {
                if connected(&app) {
                    match poll_once(&app) {
                        Ok(_) => backoff = 0,
                        Err(err) => {
                            report(&app, &err);
                            backoff = (backoff.saturating_mul(2)).clamp(120, 1800).max(120);
                        }
                    }
                }
                // 2 min connected, 10 min idle, backing off on error.
                let base = if connected(&app) { 120 } else { 600 };
                let wait = if backoff > 0 { backoff } else { base };
                for _ in 0..wait {
                    if POKE.swap(false, Ordering::Relaxed) {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
            }
        })
        .expect("failed to spawn the calendar poller");
}
