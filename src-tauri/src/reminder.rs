//! The slot's knock: a small window, shortly before a scheduled task is due.
//!
//! Every event this app puts on the calendar already carries a 10-minute
//! popup, and the phone honours it. The desktop half of that promise is here:
//! not a toast (Windows toasts cannot carry snooze buttons, and arrive subject
//! to Focus Assist and an AppUserModelID the app never registered), but the
//! same corner window the check-in uses - the app's one polite way of
//! interrupting.
//!
//! The clock lives in the expiry watcher's loop, not in any webview: the
//! dashboard is hidden most of the day and WebView2 throttles hidden timers,
//! but the tray process never sleeps for longer than thirty seconds.
//!
//! # Why the label matters
//!
//! Same constraints as the check-in: the label must **not** start with
//! `popup` (`popup::close` destroys by prefix), and it must be in the
//! `CloseRequested` exception in `lib.rs`, or closing hides it instead of
//! destroying it - an invisible window that then blocks every later reminder.

use std::sync::atomic::{AtomicI64, Ordering};

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::db::{tasks, Db};

pub const LABEL: &str = "reminder";

/// Same note-sized shape as the check-in, for the same reason: a reminder is
/// an aside, not a demand.
const WIDTH: f64 = 420.0;
const HEIGHT: f64 = 250.0;
const INSET: f64 = 24.0;

/// Minutes of warning when the setting is absent - the same ten minutes the
/// calendar event itself asks of the phone, so both devices knock together.
const DEFAULT_LEAD_MIN: i64 = 10;

/// How long past its moment a fire is still worth showing: one slot length,
/// the same 30 minutes every event occupies. Wake the machine an hour after
/// the slot ended and the reminder would not be a reminder, just an
/// accusation - those are written off silently instead of shown.
const STALE_AFTER: i64 = 30 * 60;

/// Which occurrence the open window is showing, so a pass can tell "still
/// current" from "the task moved under it". Meaningless while no window is
/// open; only ever read behind `is_open`.
static SHOWN_TASK: AtomicI64 = AtomicI64::new(0);
static SHOWN_TS: AtomicI64 = AtomicI64::new(0);

pub fn is_open(app: &AppHandle) -> bool {
    app.get_webview_window(LABEL).is_some()
}

pub fn close(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.destroy();
    }
}

/// What this pass owes one task's reminder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Due {
    /// Nothing: not yet time, or this occurrence was already handled.
    Not,
    /// Put it on screen.
    Fire,
    /// Its moment passed unseen; write it off so it never fires absurdly late.
    Stale,
}

/// The whole firing decision, pure so it can be tested to the second.
///
/// A snooze outranks the base schedule: it exists only because the base fire
/// already happened and the user asked for exactly this deferral, so it is
/// honoured even when the slot itself is behind us. Both paths go stale one
/// slot length after their moment.
fn due_now(
    now: i64,
    scheduled_ts: i64,
    fired_for_ts: Option<i64>,
    snoozed_until: Option<i64>,
    lead_secs: i64,
) -> Due {
    if let Some(snooze) = snoozed_until {
        return if now < snooze {
            Due::Not
        } else if now <= snooze + STALE_AFTER {
            Due::Fire
        } else {
            Due::Stale
        };
    }
    if fired_for_ts == Some(scheduled_ts) {
        return Due::Not; // this occurrence is spent
    }
    if now < scheduled_ts - lead_secs {
        Due::Not
    } else if now <= scheduled_ts + STALE_AFTER {
        Due::Fire
    } else {
        Due::Stale
    }
}

/// Put a reminder on screen, when one is owed and there is somewhere to put
/// it. Called from the expiry watcher every pass; every decision is re-derived
/// from the database, so schedule changes need no plumbing to reach here.
pub fn offer(app: &AppHandle) {
    let now = chrono::Utc::now().timestamp();
    let Some(db) = app.try_state::<Db>() else {
        return;
    };

    // Read everything in one visit, then let go of the lock: nothing below
    // may hold it across window work.
    let (lead_secs, rows) = {
        let Ok(conn) = db.0.lock() else {
            return;
        };
        let lead_min = crate::db::get_setting(&conn, "remind_before_min")
            .and_then(|v| v.trim().parse::<i64>().ok())
            .unwrap_or(DEFAULT_LEAD_MIN);
        let rows = tasks::reminder_rows(&conn).unwrap_or_default();
        (lead_min * 60, rows)
    };

    // Zero is the off switch, the same convention the interrupt thresholds
    // use. An open window is closed rather than orphaned mid-change.
    if lead_secs <= 0 {
        close(app);
        return;
    }

    // A window already up: keep it only while it still tells the truth. The
    // task completing, leaving Schedule or moving to a new slot all invalidate
    // it - the watcher will re-offer for the new state on a later pass.
    if is_open(app) {
        let shown_task = SHOWN_TASK.load(Ordering::Relaxed);
        let shown_ts = SHOWN_TS.load(Ordering::Relaxed);
        let current = rows
            .iter()
            .any(|row| row.id == shown_task && row.scheduled_ts == shown_ts);
        if !current {
            close(app);
        }
        return;
    }

    // Sort the pass's findings. Rows arrive earliest slot first, so the first
    // Fire is the most pressing; every Stale is written off in one visit,
    // even while paused or locked - a three-day pause must not end in a
    // morning of dead reminders.
    let mut stale = Vec::new();
    let mut fire = None;
    for row in &rows {
        match due_now(now, row.scheduled_ts, row.fired_for_ts, row.snoozed_until, lead_secs) {
            Due::Stale => stale.push(row.id),
            Due::Fire => fire = fire.or(Some((row.id, row.scheduled_ts))),
            Due::Not => {}
        }
    }

    if !stale.is_empty() {
        if let Ok(conn) = db.0.lock() {
            for id in &stale {
                let _ = tasks::mark_reminded(&conn, *id);
            }
        }
        crate::log::line(&format!(
            "reminder: wrote off {} occurrence(s) whose moment passed unseen",
            stale.len()
        ));
    }

    let Some((task_id, scheduled_ts)) = fire else {
        return;
    };

    // The check-in's gates, plus one: a pause means paused. The phone still
    // carries Google's own popup, so the safety net survives the silence.
    // Each gate defers rather than consumes - nothing is written until the
    // user acts or the occurrence goes stale, so a lock screen or a ritual in
    // progress just means the next pass asks again.
    if !crate::popup::ritual_windows(app).is_empty() || crate::checkin::is_open(app) {
        return;
    }
    if crate::triggers::workstation_locked() {
        return;
    }
    if crate::pause::paused_until_for(app).is_some() {
        return;
    }

    if let Err(err) = open(app, task_id, scheduled_ts) {
        crate::log::line(&format!("reminder: could not open the window ({err})"));
    }
}

/// One window, ever - the check-in's shape, positioned just above it would
/// be, for the same reasons written there.
fn open(app: &AppHandle, task_id: i64, scheduled_ts: i64) -> tauri::Result<()> {
    if is_open(app) {
        return Ok(());
    }

    let window = WebviewWindowBuilder::new(
        app,
        LABEL,
        WebviewUrl::App(format!("index.html?window=reminder&task={task_id}").into()),
    )
    .title("Threshold")
    .inner_size(WIDTH, HEIGHT)
    .resizable(false)
    .decorations(false)
    .always_on_top(true)
    // Findable if it ever slips behind something; it does not cover the
    // screen, so it has no reason to hide from the taskbar.
    .skip_taskbar(false)
    .visible(false)
    // The chime plays with nobody having clicked - Chromium would otherwise
    // hold it for a gesture that never comes. The feature list is wry's own
    // default, restated because setting any argument replaces it wholesale.
    .additional_browser_args(
        "--autoplay-policy=no-user-gesture-required \
         --disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection",
    )
    .build()?;

    // Bottom right of the work area, above the tray, off whatever is being
    // read - the check-in's spot.
    if let Ok(Some(monitor)) = window.primary_monitor() {
        let scale = monitor.scale_factor();
        let size = monitor.size().to_logical::<f64>(scale);
        let position = monitor.position().to_logical::<f64>(scale);
        let _ = window.set_position(tauri::LogicalPosition::new(
            position.x + size.width - WIDTH - INSET,
            position.y + size.height - HEIGHT - INSET * 2.0,
        ));
    }

    SHOWN_TASK.store(task_id, Ordering::Relaxed);
    SHOWN_TS.store(scheduled_ts, Ordering::Relaxed);

    // Shown on the main thread, and never focused.
    //
    // Both lessons are already paid for: a window shown from a worker thread
    // is created and stays invisible, and tao's focus fallback synthesises a
    // left-Alt keypress that opens the Start menu. Not taking focus is why
    // the visible Dismiss control is mandatory rather than a nicety.
    let to_show = window.clone();
    window.app_handle().run_on_main_thread(move || {
        if let Err(err) = to_show.show() {
            crate::log::line(&format!("reminder: could not be shown ({err})"));
        }
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEAD: i64 = 10 * 60;
    const SLOT_AT: i64 = 100_000;

    #[test]
    fn fires_inside_the_lead_window_and_not_before() {
        assert_eq!(due_now(SLOT_AT - LEAD - 1, SLOT_AT, None, None, LEAD), Due::Not);
        assert_eq!(due_now(SLOT_AT - LEAD, SLOT_AT, None, None, LEAD), Due::Fire);
        assert_eq!(due_now(SLOT_AT, SLOT_AT, None, None, LEAD), Due::Fire);
        assert_eq!(due_now(SLOT_AT + STALE_AFTER, SLOT_AT, None, None, LEAD), Due::Fire);
    }

    #[test]
    fn a_missed_occurrence_goes_stale_one_slot_after_its_moment() {
        assert_eq!(
            due_now(SLOT_AT + STALE_AFTER + 1, SLOT_AT, None, None, LEAD),
            Due::Stale
        );
    }

    #[test]
    fn a_spent_occurrence_stays_quiet_even_when_late() {
        let spent = Some(SLOT_AT);
        assert_eq!(due_now(SLOT_AT, SLOT_AT, spent, None, LEAD), Due::Not);
        assert_eq!(
            due_now(SLOT_AT + STALE_AFTER + 1, SLOT_AT, spent, None, LEAD),
            Due::Not,
            "spent is quieter than stale - nothing to write off either"
        );
    }

    #[test]
    fn a_marker_from_an_old_occurrence_re_arms_the_new_one() {
        let old = Some(SLOT_AT - 7 * 86_400);
        assert_eq!(due_now(SLOT_AT - LEAD, SLOT_AT, old, None, LEAD), Due::Fire);
    }

    #[test]
    fn a_snooze_fires_at_its_own_time_regardless_of_the_slot() {
        let snooze = Some(SLOT_AT + 15 * 60); // deferred past the slot start
        let spent = Some(SLOT_AT);
        assert_eq!(due_now(SLOT_AT + 14 * 60, SLOT_AT, spent, snooze, LEAD), Due::Not);
        assert_eq!(due_now(SLOT_AT + 15 * 60, SLOT_AT, spent, snooze, LEAD), Due::Fire);
        assert_eq!(
            due_now(SLOT_AT + 15 * 60 + STALE_AFTER + 1, SLOT_AT, spent, snooze, LEAD),
            Due::Stale,
            "a snooze slept through gets the same write-off as a slot"
        );
    }
}
