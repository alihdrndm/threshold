//! Phase 1 — the moments Threshold exists to catch.
//!
//! Three paths lead to the same place, because no single one is reliable:
//!   * logon    — the app starts and asks for the ritual itself
//!   * wake     — `WM_POWERBROADCAST` / `PBT_APMRESUMESUSPEND`
//!   * unlock   — `WTS_SESSION_UNLOCK`
//!
//! Modern Standby machines routinely deliver a wake as nothing but an unlock,
//! so the overlap is intentional and [`debounce`] collapses it.

pub mod debounce;
pub mod message_window;
pub mod scheduled_tasks;

use std::sync::Mutex;
use std::time::Instant;

use tauri::AppHandle;

pub use debounce::{Decision, TriggerKind, TriggerState};

static STATE: Mutex<Option<TriggerState>> = Mutex::new(None);

/// What the operating system told us. Distinct from [`TriggerKind`]: a lock or
/// a suspend is a system event that never prompts, it only records leaving.
#[derive(Debug, Clone, Copy)]
pub enum SystemEvent {
    Suspending,
    Resumed,
    Locked,
    Unlocked,
}

pub fn init(app: AppHandle) {
    // Honour the configured thresholds. They were persisted by the settings
    // panel and then never read, so changing them silently did nothing.
    // Kept rather than replaced if a trigger got here first. A relaunch handed
    // over by the single-instance plugin can arrive while setup is still
    // running; replacing the state then would forget the ritual it opened.
    let (lock_threshold, min_gap) = read_thresholds(&app);
    with_state(|state| state.set_thresholds(lock_threshold, min_gap));
    message_window::spawn(app);
}

/// Re-read the thresholds after Settings changed them, keeping everything the
/// debounce already remembers. Called by the settings command, so a change
/// takes effect on the very next trigger rather than after a restart.
pub fn reload_thresholds(app: &AppHandle) {
    let (lock_threshold, min_gap) = read_thresholds(app);
    with_state(|state| state.set_thresholds(lock_threshold, min_gap));
}

/// The two thresholds, in seconds, from settings.
///
/// Stored in seconds under `_sec` keys. The older `_min` keys are read as a
/// fallback and converted, so a value someone set last month still means
/// what it meant. Zero is allowed and means "always": no gap, or any lock
/// counts as leaving.
fn read_thresholds(app: &AppHandle) -> (std::time::Duration, std::time::Duration) {
    use tauri::Manager;
    let read = |sec_key: &str, min_key: &str, default: std::time::Duration| {
        app.try_state::<crate::db::Db>()
            .and_then(|db| {
                db.0.lock().ok().and_then(|conn| {
                    crate::db::get_setting(&conn, sec_key)
                        .and_then(|raw| raw.trim().parse::<u64>().ok())
                        .or_else(|| {
                            crate::db::get_setting(&conn, min_key)
                                .and_then(|raw| raw.trim().parse::<u64>().ok())
                                .map(|minutes| minutes * 60)
                        })
                })
            })
            .map(std::time::Duration::from_secs)
            .unwrap_or(default)
    };
    (
        read(
            "unlock_threshold_sec",
            "unlock_threshold_min",
            debounce::DEFAULT_LOCK_THRESHOLD,
        ),
        read("min_gap_sec", "min_gap_min", debounce::DEFAULT_MIN_GAP),
    )
}

fn handle_system_event(app: &AppHandle, event: SystemEvent) {
    let now = Instant::now();

    let kind = match event {
        SystemEvent::Locked => {
            with_state(|state| state.mark_locked(now));
            return;
        }
        SystemEvent::Suspending => {
            with_state(|state| state.mark_suspended());
            return;
        }
        SystemEvent::Resumed => {
            // A wake is a good moment to reconcile the calendar: the machine
            // may have slept through a change made on the phone.
            crate::calendar::sync::request_poll();
            TriggerKind::Wake
        }
        SystemEvent::Unlocked => TriggerKind::Unlock,
    };

    request(app, kind);
}

/// Open the ritual for a specific task, reporting why if it cannot.
///
/// Returns the reason rather than swallowing it, so "Focus on this" can explain
/// itself instead of looking broken.
pub fn request_focus(app: &AppHandle, prefill: crate::popup::Prefill) -> Result<(), String> {
    if is_paused(app) {
        return Err("Threshold is paused. Resume it in Settings to start a session.".into());
    }

    let now = Instant::now();
    match with_state(|state| state.evaluate(TriggerKind::Manual, now)) {
        Decision::Show => {
            with_state(|state| state.mark_shown(now));
            crate::popup::show_with_prefill(app, TriggerKind::Manual, prefill).map_err(|err| {
                with_state(|state| state.forget_last_shown());
                format!("could not open the ritual: {err}")
            })
        }
        Decision::SessionInProgress => Err(
            "A focus session is already running. It will end on its own, or use the tray to \
             unlock early."
                .into(),
        ),
        other => Err(format!("not right now ({other:?})")),
    }
}

/// The single funnel every trigger passes through, so the debounce rules cannot
/// be bypassed by adding a new caller.
pub fn request(app: &AppHandle, kind: TriggerKind) {
    // A pause means paused. Interrupting someone who explicitly asked for a
    // break is how a tool earns an uninstall rather than a return.
    if is_paused(app) {
        crate::log::line(&format!("{kind:?}: skipped, Threshold is paused"));
        return;
    }

    // A ritual cannot be shown to a locked screen, and building a fullscreen
    // window there both wastes the prompt and unsettles the shell on the way
    // back in. Waiting costs nothing: the unlock that follows is its own
    // trigger, and it arrives at the moment the user can actually see it.
    // Not applied to Unlock: that event means the screen has just been
    // unlocked, and checking would race the desktop switching back.
    if matches!(kind, TriggerKind::Boot | TriggerKind::Wake) && workstation_locked() {
        crate::log::line(&format!(
            "{kind:?}: the screen is locked, waiting for the unlock to show it"
        ));
        return;
    }

    let now = Instant::now();
    let decision = with_state(|state| state.evaluate(kind, now));

    match decision {
        Decision::Show => {
            // Record it as shown only if it genuinely reached the screen. A
            // trigger that arrives while the session is locked cannot display
            // anything; marking it shown anyway meant the unlock that followed
            // was dismissed as too soon, and the user saw nothing at all.
            with_state(|state| state.mark_shown(now));
            crate::log::line(&format!("{kind:?}: opening the ritual"));
            if let Err(err) = crate::popup::show(app, kind) {
                crate::log::line(&format!("{kind:?}: could not open the ritual: {err}"));
                with_state(|state| state.forget_last_shown());
            }
        }
        // Each of these is correct behaviour, and each one previously looked
        // exactly like the app being broken.
        Decision::TooSoon => {
            let gap = with_state(|state| state.min_gap().as_secs());
            crate::log::line(&format!(
                "{kind:?}: suppressed, a ritual was shown less than {gap} seconds ago (see Settings)"
            ));
        }
        Decision::NotAwayLongEnough => crate::log::line(&format!(
            "{kind:?}: suppressed, the screen was not locked long enough to count as returning \
             (see Settings)"
        )),
        Decision::SessionInProgress => {
            crate::log::line(&format!("{kind:?}: suppressed, a focus session is running"))
        }
        Decision::SameReturn => crate::log::line(&format!(
            "{kind:?}: suppressed, the same return a ritual was just shown for (no lock or \
             sleep since)"
        )),
    }
}

/// Record that a focus session is running, so the ritual does not interrupt one.
///
/// Takes a deadline rather than a duration, deliberately. The duration form is
/// the one you reach for at startup, and calling it there with a stored
/// `duration_min` re-arms a full session on every launch — which would suppress
/// the boot ritual on the first launch after a week away, the single most
/// valuable trigger the product has.
pub fn note_session_until(ends_ts: i64, enforced: bool) {
    let seconds = ends_ts - chrono::Utc::now().timestamp();
    if seconds <= 0 {
        return;
    }
    with_state(|state| {
        state.start_session(debounce::Session {
            until: Instant::now() + std::time::Duration::from_secs(seconds as u64),
            ends_ts,
            enforced,
        })
    });
}

/// A commitment has ended, so triggers may prompt again.
pub fn clear_session() {
    with_state(|state| state.end_session());
}

/// Forget that a ritual was shown, because it never actually appeared.
///
/// Called by the popup watchdog. Without it, a ritual that failed to display
/// still consumed the debounce window, so the unlock that followed a locked-
/// screen wake was dismissed as too soon and nothing was ever seen.
pub fn forget_last_shown() {
    with_state(|state| state.forget_last_shown());
}

/// Is the workstation locked right now?
///
/// `OpenInputDesktop` fails while the secure (lock screen) desktop is active,
/// which is the standard way to ask. It matters because a ritual built behind
/// the lock screen is never seen: it cannot be displayed, and creating a
/// fullscreen window there also disturbs the shell on the way back in.
///
/// This replaces guessing with a time window. How long somebody takes to type
/// their password is not something to estimate.
pub(crate) fn workstation_locked() -> bool {
    use windows::Win32::System::StationsAndDesktops::{
        CloseDesktop, OpenInputDesktop, DESKTOP_ACCESS_FLAGS, DESKTOP_CONTROL_FLAGS,
    };
    unsafe {
        match OpenInputDesktop(DESKTOP_CONTROL_FLAGS(0), false, DESKTOP_ACCESS_FLAGS(0x0001)) {
            Ok(desktop) => {
                let _ = CloseDesktop(desktop);
                false
            }
            Err(_) => true,
        }
    }
}

fn is_paused(app: &AppHandle) -> bool {
    use tauri::Manager;
    app.try_state::<crate::db::Db>()
        .and_then(|db| {
            db.0.lock()
                .ok()
                .map(|conn| crate::pause::paused_until(&conn).is_some())
        })
        .unwrap_or(false)
}

/// Run `f` against the trigger state, creating it with the defaults if `init`
/// has not run yet.
///
/// It used to answer "no state" with "show", unrecorded, so a trigger that
/// beat `init` opened a ritual nothing remembered, and the boot trigger
/// straight after it opened a second.
fn with_state<T>(f: impl FnOnce(&mut TriggerState) -> T) -> T {
    let mut guard = STATE.lock().expect("trigger state poisoned");
    f(guard.get_or_insert_with(TriggerState::default))
}

/// Map a `--trigger=` argument onto a kind. Used by the scheduled tasks and by
/// the autostart entry, which relaunch the exe rather than talking to it.
pub fn kind_from_args(args: &[String]) -> Option<TriggerKind> {
    args.iter().find_map(|arg| match arg.as_str() {
        "--trigger=boot" | "--autostart" => Some(TriggerKind::Boot),
        "--trigger=wake" => Some(TriggerKind::Wake),
        "--trigger=unlock" => Some(TriggerKind::Unlock),
        // Open the ritual now, the same as the tray item. Useful for support -
        // "does it appear at all?" is a different question from "does the
        // trigger fire?", and separating them is how the stale-window bug was
        // finally pinned down.
        "--ritual" => Some(TriggerKind::Manual),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The double ritual at every boot. Setup registers the tray icon before
    /// `init`, and registering one waits on Explorer, which at boot is slow;
    /// while it waits, Windows delivers the single-instance plugin's message
    /// from the second launch. Its boot trigger found no state, was shown and
    /// not recorded, and `init` then replaced the state wholesale - so setup's
    /// own boot trigger showed the ritual again.
    ///
    /// The only test that touches the global state.
    #[test]
    fn a_trigger_that_beats_init_is_still_remembered() {
        *STATE.lock().unwrap() = None;
        let now = Instant::now();

        // The relaunch handed over during setup.
        assert_eq!(
            with_state(|state| state.evaluate(TriggerKind::Boot, now)),
            Decision::Show
        );
        with_state(|state| state.mark_shown(now));

        // What `init` does once setup reaches it, with the gap at its lowest.
        with_state(|state| {
            state.set_thresholds(std::time::Duration::ZERO, std::time::Duration::ZERO)
        });

        // Setup's own boot trigger.
        assert_eq!(
            with_state(|state| state.evaluate(TriggerKind::Boot, Instant::now())),
            Decision::SameReturn
        );
    }
}
