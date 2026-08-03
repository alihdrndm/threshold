use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};

use crate::{db::Db, pause, request_ritual, show_main};

/// The tray is Threshold's only permanent surface.
///
/// "Pause" sits here as a first-class item rather than buried in settings,
/// because a break that is hard to take gets taken by uninstalling instead.
pub fn create_tray(app: &AppHandle) -> tauri::Result<()> {
    let focus = MenuItem::with_id(app, "start-session", "Start a focus session", true, None::<&str>)?;
    let open = MenuItem::with_id(app, "open-dashboard", "Open dashboard", true, None::<&str>)?;

    let pause_menu = Submenu::with_items(
        app,
        "Pause Threshold",
        true,
        &[
            &MenuItem::with_id(app, "pause-1", "For a day", true, None::<&str>)?,
            &MenuItem::with_id(app, "pause-3", "For three days", true, None::<&str>)?,
            &MenuItem::with_id(app, "pause-7", "For a week", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "resume", "Resume now", true, None::<&str>)?,
        ],
    )?;

    let quit = MenuItem::with_id(app, "quit", "Quit Threshold", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &focus,
            &open,
            &PredefinedMenuItem::separator(app)?,
            &pause_menu,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    TrayIconBuilder::with_id("threshold-tray")
        .icon(app.default_window_icon().expect("bundled icon").clone())
        .tooltip("Threshold")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "start-session" => request_ritual(app),
            "open-dashboard" => show_main(app),
            "pause-1" => set_pause(app, 1),
            "pause-3" => set_pause(app, 3),
            "pause-7" => set_pause(app, 7),
            "resume" => clear_pause(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

fn set_pause(app: &AppHandle, days: i64) {
    if let Some(db) = app.try_state::<Db>() {
        if let Ok(conn) = db.0.lock() {
            match pause::pause_for_days(&conn, days) {
                Ok(_) => update_tooltip(app, &format!("Threshold - paused for {days}d")),
                Err(err) => eprintln!("could not pause: {err}"),
            }
        }
    }
}

fn clear_pause(app: &AppHandle) {
    if let Some(db) = app.try_state::<Db>() {
        if let Ok(conn) = db.0.lock() {
            let _ = pause::resume(&conn);
            update_tooltip(app, "Threshold");
        }
    }
}

fn update_tooltip(app: &AppHandle, text: &str) {
    if let Some(tray) = app.tray_by_id("threshold-tray") {
        let _ = tray.set_tooltip(Some(text));
    }
}
