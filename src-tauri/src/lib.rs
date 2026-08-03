mod startup;
mod tray;
mod triggers;

use tauri::{Manager, RunEvent};

/// Round-trip probe for the IPC bridge. Phase 0 acceptance uses it; keep it
/// until real commands exist.
#[tauri::command]
fn ping() -> String {
    "Core responded.".to_string()
}

pub fn run() {
    tauri::Builder::default()
        // Single-instance must be registered first: it decides whether this
        // process lives at all, and later phases relaunch threshold.exe from
        // scheduled tasks, which land in this callback rather than starting a
        // second app.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // Phase 1 will match on _argv here (--trigger=unlock, --autostart)
            // to decide between showing the dashboard and firing the ritual.
            show_main(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent, // ignored on Windows
            Some(vec!["--autostart"]),
        ))
        .invoke_handler(tauri::generate_handler![ping])
        .setup(|app| {
            startup::enable_autostart(app.handle());
            tray::create_tray(app.handle())?;
            Ok(())
        })
        // Closing a window hides it. Threshold is a resident tray app; an X
        // click means "get out of my way", not "stop intercepting".
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!())
        .expect("failed to build Threshold")
        .run(|_app, event| {
            // With every window hidden Tauri would otherwise exit. Only an
            // explicit app.exit() (tray → Quit) should end the process.
            if let RunEvent::ExitRequested { api, code, .. } = event {
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}

pub(crate) fn show_main(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}
