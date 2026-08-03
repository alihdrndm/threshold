//! Launching at logon.
//!
//! Two failure modes this deliberately avoids:
//!
//! 1. **A stale path.** Registering only when nothing is registered means the
//!    entry keeps pointing at wherever the exe used to live - a development
//!    build, or a previous install location. It silently launches the wrong
//!    binary, or none at all.
//! 2. **Overriding the user.** Re-enabling on every launch means turning
//!    autostart off never sticks. The preference is stored and honoured.

use tauri::Manager;
use tauri_plugin_autostart::ManagerExt;

use crate::db::{self, Db};

const KEY: &str = "launch_at_login";
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "Threshold";

/// Whether the user wants Threshold at logon. Defaults to on: an interceptor
/// that is not running has nothing to intercept.
pub fn wants_autostart(app: &tauri::AppHandle) -> bool {
    app.try_state::<Db>()
        .and_then(|state| {
            state.0.lock().ok().map(|conn| {
                db::get_setting(&conn, KEY)
                    .map(|value| value != "false")
                    .unwrap_or(true)
            })
        })
        .unwrap_or(true)
}

/// Bring the registered entry in line with both the preference and the current
/// executable path.
pub fn sync_autostart(app: &tauri::AppHandle) {
    let manager = app.autolaunch();
    let wanted = wants_autostart(app);
    let enabled = manager.is_enabled().unwrap_or(false);

    if !wanted {
        if enabled {
            if let Err(err) = manager.disable() {
                eprintln!("could not disable autostart: {err}");
            }
        }
        return;
    }

    // Re-register whenever the recorded path is not this executable, so moving
    // or rebuilding the app cannot leave a logon entry aimed at a binary that
    // no longer exists.
    if enabled && registered_path_matches_current() {
        return;
    }

    let _ = manager.disable();
    if let Err(err) = manager.enable() {
        eprintln!("could not enable autostart: {err}");
    }
}

#[cfg(windows)]
fn registered_path_matches_current() -> bool {
    let Ok(current) = std::env::current_exe() else {
        return true; // Cannot tell; leave whatever is there alone.
    };
    let current = current.to_string_lossy().to_lowercase();

    let output = std::process::Command::new("reg")
        .args(["query", &format!(r"HKCU\{RUN_KEY}"), "/v", RUN_VALUE])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout).to_lowercase();
            text.contains(&current)
        }
        // No entry, or the query failed: treat as needing registration.
        _ => false,
    }
}

#[cfg(not(windows))]
fn registered_path_matches_current() -> bool {
    true
}
