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

/// What the operating system told us. Distinct from [`TriggerKind`]: a lock is
/// a system event that never prompts, it only starts the away timer.
#[derive(Debug, Clone, Copy)]
pub enum SystemEvent {
    Resumed,
    Locked,
    Unlocked,
}

pub fn init(app: AppHandle) {
    // Honour the configured lock threshold. It was persisted by the settings
    // panel and then never read, so changing it silently did nothing.
    let threshold = read_lock_threshold(&app);
    *STATE.lock().expect("trigger state poisoned") =
        Some(TriggerState::with_lock_threshold(threshold));
    message_window::spawn(app);
}

fn read_lock_threshold(app: &AppHandle) -> std::time::Duration {
    use tauri::Manager;
    let minutes = app
        .try_state::<crate::db::Db>()
        .and_then(|db| {
            db.0.lock().ok().and_then(|conn| {
                crate::db::get_setting(&conn, "unlock_threshold_min")
                    .and_then(|raw| raw.parse::<u64>().ok())
            })
        })
        .filter(|minutes| *minutes > 0)
        .unwrap_or(20);
    std::time::Duration::from_secs(minutes * 60)
}

fn handle_system_event(app: &AppHandle, event: SystemEvent) {
    let now = Instant::now();

    let kind = match event {
        SystemEvent::Locked => {
            with_state(|state| state.mark_locked(now));
            return;
        }
        SystemEvent::Resumed => TriggerKind::Wake,
        SystemEvent::Unlocked => TriggerKind::Unlock,
    };

    request(app, kind);
}

/// Open the ritual for a specific task, reporting why if it cannot.
///
/// Returns the reason rather than swallowing it, so "Focus on this" can explain
/// itself instead of looking broken.
pub fn request_with_intent(app: &AppHandle, intent: String) -> Result<(), String> {
    if is_paused(app) {
        return Err("Threshold is paused. Resume it in Settings to start a session.".into());
    }

    let now = Instant::now();
    match with_state(|state| state.evaluate(TriggerKind::Manual, now)).unwrap_or(Decision::Show) {
        Decision::Show => {
            with_state(|state| state.mark_shown(now));
            crate::popup::show_with_intent(app, TriggerKind::Manual, Some(intent)).map_err(|err| {
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
    let decision = with_state(|state| state.evaluate(kind, now)).unwrap_or(Decision::Show);

    match decision {
        Decision::Show => {
            // Record it as shown only if it genuinely reached the screen. A
            // trigger that arrives while the session is locked cannot display
            // anything; marking it shown anyway meant the unlock that followed
            // was dismissed as too soon, and the user saw nothing at all.
            // The kind is recorded too: an unlock straight after a wake is the
            // same return to the machine, and the wake's ritual was built
            // behind the lock screen where nobody could see it.
            with_state(|state| state.mark_shown_for(kind, now));
            crate::log::line(&format!("{kind:?}: opening the ritual"));
            if let Err(err) = crate::popup::show(app, kind) {
                crate::log::line(&format!("{kind:?}: could not open the ritual: {err}"));
                with_state(|state| state.forget_last_shown());
            }
        }
        // Each of these is correct behaviour, and each one previously looked
        // exactly like the app being broken.
        Decision::TooSoon => crate::log::line(&format!(
            "{kind:?}: suppressed, a ritual was shown less than {} minutes ago",
            debounce::MIN_GAP.as_secs() / 60
        )),
        Decision::NotAwayLongEnough => crate::log::line(&format!(
            "{kind:?}: suppressed, the screen was not locked long enough to count as returning \
             (see Settings)"
        )),
        Decision::SessionInProgress => {
            crate::log::line(&format!("{kind:?}: suppressed, a focus session is running"))
        }
    }
}

/// Record that a focus session is running, so the ritual does not interrupt one.
///
/// Previously `start_session` existed but was never called, which meant a wake
/// mid-session re-prompted as if nothing were underway.
pub fn note_session(duration_min: i64) {
    let until = Instant::now() + std::time::Duration::from_secs((duration_min.max(0) * 60) as u64);
    with_state(|state| state.start_session(until));
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
fn workstation_locked() -> bool {
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

fn with_state<T>(f: impl FnOnce(&mut TriggerState) -> T) -> Option<T> {
    let mut guard = STATE.lock().expect("trigger state poisoned");
    guard.as_mut().map(f)
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
