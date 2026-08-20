mod blocked;
mod calendar;
mod checkin;
mod commands;
mod db;
pub mod diagnostics;
mod log;
mod pause;
mod popup;
mod session;
mod startup;
mod tray;
pub mod triggers;
mod wall;

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
            commands::remembered_sites,
            commands::remember_sites,
            commands::check_site,
            commands::list_quotes,
            commands::add_quote,
            commands::remove_quote,
            commands::quote_for,
            commands::choose_quote,
            commands::data_location,
            commands::open_dashboard,
            commands::list_contexts,
            commands::add_context,
            commands::rename_context,
            commands::remove_context,
            commands::set_task_context,
            commands::list_tasks,
            commands::add_task,
            commands::move_task,
            commands::reorder_tasks,
            commands::set_task_status,
            commands::focus_on_task,
            commands::pause_status,
            commands::pause_for,
            commands::resume_now,
            commands::get_settings,
            commands::set_setting,
            commands::diagnostics,
            commands::repair_helper,
            commands::set_window_theme,
            commands::emergency_unblock,
            commands::session_status,
            commands::recent_sessions,
            commands::end_session_early,
            commands::pending_checkin,
            commands::answer_checkin,
            commands::continue_session,
            commands::start_again,
            commands::schedule_task,
            commands::clear_history,
            commands::dismiss_wall,
            commands::calendar_status,
            commands::google_connect,
            commands::google_disconnect,
            commands::calendar_sync_now,
            commands::reschedule_task,
            commands::unschedule_task,
            commands::open_url,
            commands::calendar_week,
            commands::dismiss_checkin,
        ])
        .setup(|app| {
            let handle = app.handle().clone();

            // Without a database the ritual cannot record anything, which is
            // the one thing it must not fail silently at.
            let conn = db::open().map_err(|err| format!("database unavailable: {err}"))?;
            app.manage(db::Db(Mutex::new(conn)));
            app.manage(calendar::week::WeekCache::default());

            startup::sync_autostart(&handle);

            // Register the relaunch tasks if they are missing or aimed at a
            // different build. Without this a fresh install has no triggers at
            // all, which is precisely how "I slept the machine and nothing
            // happened" occurred.
            scheduled_tasks::ensure_registered();

            tray::create_tray(&handle)?;
            triggers::init(handle.clone());

            // Close whatever finished while we were shut, and re-arm whatever
            // did not. Must come after triggers::init, which replaces the
            // trigger state wholesale, and before the boot trigger below -
            // otherwise the logon task walks straight into a fullscreen ritual
            // on top of a commitment that is still running.
            session::reconcile_on_startup(&handle);

            // A commitment that ran out while the app was closed must still be
            // lifted, so this watches the lock rather than a timer held in the
            // memory of whichever run armed it.
            session::spawn_expiry_watcher(handle.clone());

            // Blocked names resolve to an address this app owns, so a browser
            // that tries one lands here and can be answered with a line the user
            // chose. Idle sockets until then, and entirely optional: if the
            // ports cannot be bound, blocking is unaffected and only the quote
            // is lost.
            wall::listen(&handle);

            // The calendar reconciler: quiet until the user connects Google in
            // Settings, then it keeps scheduled tasks and their events in step.
            calendar::init(&handle);

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
                // The check-in is destroyed rather than hidden, for the same
                // reason the ritual is: a hidden window of this kind can be
                // asked to show all day and will not appear, and the next
                // session would then owe a question with nowhere to ask it.
                let transient = window.label().starts_with(popup::POPUP_LABEL)
                    || window.label() == checkin::LABEL
                    || window.label() == blocked::LABEL;
                if !transient {
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
        let me = std::env::current_exe()
            .map(|p| p.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        for (name, target) in scheduled_tasks::registered_targets() {
            match target {
                None => println!("{name}: absent"),
                Some(path) => {
                    // A task pointing at a different build is the most likely
                    // reason triggers appear not to fire.
                    let stale = !path.to_lowercase().contains(me.trim_end_matches(".exe"));
                    println!(
                        "{name}: registered -> {path}{}",
                        if stale { "   [not this build]" } else { "" }
                    );
                }
            }
        }
        return true;
    }

    // Install-time: register the one elevated task. Needs administrator rights,
    // and is the only UAC prompt Threshold ever causes.
    if let Some(arg) = args.iter().find(|a| a.starts_with("--register-helper")) {
        let helper = arg
            .split_once('=')
            .map(|(_, path)| path.to_string())
            .unwrap_or_else(|| {
                scheduled_tasks::helper_path()
                    .to_string_lossy()
                    .to_string()
            });
        match scheduled_tasks::register_helper(&helper) {
            Ok(()) => println!("registered {} -> {helper}", scheduled_tasks::HELPER_TASK),
            Err(err) => eprintln!("could not register the helper task: {err}"),
        }
        return true;
    }

    if args.iter().any(|a| a == "--unregister-helper") {
        match scheduled_tasks::unregister_helper() {
            Ok(()) => println!("removed {}", scheduled_tasks::HELPER_TASK),
            Err(err) => eprintln!("could not remove the helper task: {err}"),
        }
        return true;
    }

    if args.iter().any(|a| a == "--run-helper") {
        match scheduled_tasks::run_helper() {
            Ok(()) => println!("helper task started"),
            Err(err) => eprintln!("could not start the helper task: {err}"),
        }
        return true;
    }

    // One place that says whether this installation can actually do its job.
    // Every failure the user hit was silent; this makes them all visible.
    if args.iter().any(|a| a == "--doctor") {
        println!("{}", diagnostics::diagnostics().report());
        return true;
    }

    // What the app has actually been deciding, since the shipped build has no
    // console to print it to.
    if args.iter().any(|a| a == "--log") {
        let lines = log::tail(60);
        if lines.is_empty() {
            println!("nothing logged yet");
        } else {
            for line in lines {
                println!("{line}");
            }
        }
        if let Some(path) = log::path() {
            println!("\n(full log: {})", path.display());
        }
        return true;
    }

    // The escape hatch. Deliberately the emergency path rather than the ordinary
    // lift: someone typing this has already decided, and an ordinary unblock is
    // refused for exactly as long as the commitment they are trying to leave.
    // Being told "unblocked" and staying blocked is how this read before.
    if args.iter().any(|a| a == "--unblock-now") {
        match session::emergency_unblock() {
            Ok(()) => println!("unblocked"),
            Err(err) => eprintln!("could not unblock: {err}"),
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
                        "{} | {} | trigger={} | predicted_yes={:?} | {} min | blocks=[{}] | task={} | {}",
                        row.ts,
                        row.outcome,
                        row.trigger.as_deref().unwrap_or("-"),
                        row.predicted_yes,
                        row.duration_min.map(|d| d.to_string()).unwrap_or("-".into()),
                        row.categories.as_deref().unwrap_or(""),
                        row.task_id.map(|id| id.to_string()).unwrap_or("-".into()),
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

/// Show the dashboard, creating it the first time it is asked for.
///
/// Created on demand rather than declared in the config: a declared window
/// starts a WebView2 instance at launch - six processes and several hundred
/// megabytes - for a window that spends most days unopened. Threshold is
/// supposed to sit in the tray costing almost nothing.
pub(crate) fn show_main(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        return;
    }

    // The document's own colours are handled by the bootstrap in index.html;
    // this is only the native chrome around them. `None` means follow Windows,
    // which is also what leaves prefers-color-scheme free for the frontend.
    let theme = app
        .try_state::<db::Db>()
        .and_then(|state| {
            state
                .0
                .lock()
                .ok()
                .and_then(|conn| db::get_setting(&conn, "appearance"))
        })
        .and_then(|value| match value.as_str() {
            "light" => Some(tauri::Theme::Light),
            "dark" => Some(tauri::Theme::Dark),
            _ => None,
        });

    let built = tauri::WebviewWindowBuilder::new(
        app,
        "main",
        tauri::WebviewUrl::App("index.html".into()),
    )
    .theme(theme)
    .title("Threshold")
    .inner_size(1180.0, 780.0)
    .min_inner_size(880.0, 600.0)
    .center()
    // Ctrl +/-/0 to resize everything, as in any other Windows app. Off by
    // default in Tauri, and its absence is the kind of thing that quietly makes
    // an app unusable for anyone who needs larger text.
    .zoom_hotkeys_enabled(true)
    .build();

    match built {
        Ok(window) => {
            let _ = window.set_focus();
        }
        Err(err) => eprintln!("could not open the dashboard: {err}"),
    }
}

pub(crate) fn request_ritual(app: &tauri::AppHandle) {
    triggers::request(app, TriggerKind::Manual);
}

/// Re-run helper registration with administrator rights.
///
/// Registering a "run with highest privileges" task itself requires elevation,
/// so this is the one action the app cannot complete on its own. `runas` raises
/// the standard UAC prompt; if the user declines, nothing changes.
pub(crate) fn elevate_register_helper() -> Result<(), String> {
    let exe = std::env::current_exe()
        .map_err(|err| format!("could not resolve own path: {err}"))?;
    let helper = scheduled_tasks::helper_path();
    if !helper.is_file() {
        return Err(format!(
            "the helper binary is missing at {}. Reinstalling Threshold will restore it.",
            helper.display()
        ));
    }

    let status = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &format!(
                "Start-Process -FilePath '{}' -ArgumentList '--register-helper={}' -Verb RunAs -Wait",
                exe.display(),
                helper.display()
            ),
        ])
        .status()
        .map_err(|err| format!("could not request administrator rights: {err}"))?;

    if !status.success() {
        return Err("administrator rights were declined".into());
    }
    if !scheduled_tasks::helper_usable() {
        return Err("registration did not take effect".into());
    }
    Ok(())
}
