mod commands;
mod db;
mod popup;
mod startup;
mod tray;
mod triggers;

use std::sync::Mutex;

use tauri::{Manager, RunEvent};

use triggers::{scheduled_tasks, TriggerKind};

pub fn run() {
    tauri::Builder::default()
        // Single-instance must be registered first: it decides whether this
        // process lives at all. The relaunch scheduled tasks and the autostart
        // entry both re-exec the exe, so their arguments arrive here rather
        // than starting a second app.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            println!("second instance: argv={argv:?}");
            match triggers::kind_from_args(&argv) {
                Some(kind) => triggers::request(app, kind),
                None => show_main(app),
            }
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent, // ignored on Windows
            Some(vec!["--autostart"]),
        ))
        .invoke_handler(tauri::generate_handler![
            commands::ping,
            commands::dismiss_popup,
            commands::finish_ritual,
            commands::intention_suggestions,
            commands::recent_intentions,
            commands::remembered_categories,
            commands::remember_categories,
            commands::data_location,
            commands::open_dashboard,
        ])
        .setup(|app| {
            let handle = app.handle().clone();

            // Without a database the ritual cannot record anything, which is
            // the one thing it must not fail silently at.
            let conn = db::open().map_err(|err| format!("database unavailable: {err}"))?;
            app.manage(db::Db(Mutex::new(conn)));

            startup::enable_autostart(&handle);
            tray::create_tray(&handle)?;
            triggers::init(handle.clone());

            // Launched by the logon task or the autostart entry: this *is* the
            // boot moment, so ask for the ritual straight away.
            let args: Vec<String> = std::env::args().collect();
            if let Some(kind) = triggers::kind_from_args(&args) {
                triggers::request(&handle, kind);
            }

            Ok(())
        })
        // Closing a window hides it. Threshold is a resident tray app; an X
        // click means "get out of my way", not "stop intercepting". The popup
        // is the exception - it is meant to be dismissed and rebuilt.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() != popup::POPUP_LABEL {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("failed to build Threshold")
        .run(|_app, event| {
            // With every window hidden Tauri would otherwise exit. Only an
            // explicit app.exit() (tray -> Quit) should end the process.
            if let RunEvent::ExitRequested { api, code, .. } = event {
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}

/// Handle `--register-tasks` / `--unregister-tasks` before any window exists.
/// Returns true when the process should exit instead of starting the app.
pub fn handle_task_cli(args: &[String]) -> bool {
    if args.iter().any(|a| a == "--register-tasks") {
        match scheduled_tasks::register_all() {
            Ok(names) => println!("registered: {}", names.join(", ")),
            Err(err) => eprintln!("could not register tasks: {err}"),
        }
        return true;
    }

    if args.iter().any(|a| a == "--unregister-tasks") {
        for (name, result) in scheduled_tasks::unregister_all() {
            match result {
                Ok(()) => println!("removed: {name}"),
                Err(err) => eprintln!("could not remove {name}: {err}"),
            }
        }
        return true;
    }

    if args.iter().any(|a| a == "--task-status") {
        for (name, exists) in scheduled_tasks::status() {
            println!("{name}: {}", if exists { "registered" } else { "absent" });
        }
        return true;
    }

    // Reading your own history should not require a SQLite client.
    if args.iter().any(|a| a == "--recent") {
        match db::open().and_then(|conn| db::intentions::recent(&conn, 20)) {
            Ok(rows) if rows.is_empty() => println!("no intentions recorded yet"),
            Ok(rows) => {
                for row in rows {
                    println!(
                        "{} | {} | trigger={} | predicted_yes={:?} | {} min | blocks=[{}] | {}",
                        row.ts,
                        row.outcome,
                        row.trigger.as_deref().unwrap_or("-"),
                        row.predicted_yes,
                        row.duration_min.map(|d| d.to_string()).unwrap_or("-".into()),
                        row.categories.as_deref().unwrap_or(""),
                        row.text.as_deref().unwrap_or("(no text)"),
                    );
                }
            }
            Err(err) => eprintln!("could not read history: {err}"),
        }
        return true;
    }

    false
}

pub(crate) fn show_main(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

pub(crate) fn request_ritual(app: &tauri::AppHandle) {
    triggers::request(app, TriggerKind::Manual);
}
