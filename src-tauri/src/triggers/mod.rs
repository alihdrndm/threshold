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
    *STATE.lock().expect("trigger state poisoned") = Some(TriggerState::default());
    message_window::spawn(app);
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

/// The single funnel every trigger passes through, so the debounce rules cannot
/// be bypassed by adding a new caller.
pub fn request(app: &AppHandle, kind: TriggerKind) {
    let now = Instant::now();
    let decision = with_state(|state| state.evaluate(kind, now)).unwrap_or(Decision::Show);

    match decision {
        Decision::Show => {
            with_state(|state| state.mark_shown(now));
            if let Err(err) = crate::popup::show(app, kind) {
                eprintln!("triggers: could not open the ritual: {err}");
            }
        }
        other => {
            // Phase 2 turns SessionInProgress into a small "X min left" toast.
            println!("triggers: {kind:?} suppressed ({other:?})");
        }
    }
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
        _ => None,
    })
}
