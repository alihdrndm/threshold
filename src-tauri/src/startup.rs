use tauri_plugin_autostart::ManagerExt;

/// Launch at logon via an HKCU Run key — no elevation, and enough for Phase 0.
///
/// This is temporary by design. The spec wants a scheduled task with a ~10s
/// logon delay (so the ritual lands after the shell settles), registered during
/// the one-and-only UAC consent at install. That arrives with the rest of the
/// Task Scheduler work; isolating the mechanism here keeps the swap to one file.
pub fn enable_autostart(app: &tauri::AppHandle) {
    let manager = app.autolaunch();
    if !manager.is_enabled().unwrap_or(false) {
        if let Err(err) = manager.enable() {
            eprintln!("could not enable autostart: {err}");
        }
    }
}
