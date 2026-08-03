use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle,
};

use crate::{request_ritual, show_main};

/// The tray is Threshold's only permanent surface. Phase 4/5 add: pause for N
/// days, emergency unlock, settings - and a countdown in the tooltip while a
/// session runs, which is why the icon keeps a stable id.
pub fn create_tray(app: &AppHandle) -> tauri::Result<()> {
    let focus = MenuItem::with_id(app, "start-session", "Start a focus session", true, None::<&str>)?;
    let open = MenuItem::with_id(app, "open-dashboard", "Open dashboard", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Threshold", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&focus, &open, &separator, &quit])?;

    TrayIconBuilder::with_id("threshold-tray")
        .icon(app.default_window_icon().expect("bundled icon").clone())
        .tooltip("Threshold")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "start-session" => request_ritual(app),
            "open-dashboard" => show_main(app),
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
