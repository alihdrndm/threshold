//! The closing question: a small window, once, in the corner.
//!
//! Deliberately not the ritual. The ritual is fullscreen because it stands at a
//! threshold — you have just arrived and nothing is decided. A check-in is a
//! report on something already finished, and taking over the whole screen at
//! the *end* of work is punishment for finishing. People learn to avoid that by
//! never letting a session end cleanly.
//!
//! # Why the label matters
//!
//! It must **not** start with `popup`: `popup::close` destroys every window
//! whose label does, and `finish_ritual` calls it. But it must also be listed
//! in the `CloseRequested` exception in `lib.rs`, or the handler there hides it
//! instead of destroying it — leaving exactly the invisible, unusable window
//! that took three attempts to eliminate from the ritual.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub const LABEL: &str = "checkin";

/// Logical size. Enough for two lines, three answers and a way out; small
/// enough that it reads as a note rather than a demand.
const WIDTH: f64 = 420.0;
const HEIGHT: f64 = 250.0;
const INSET: f64 = 24.0;

pub fn is_open(app: &AppHandle) -> bool {
    app.get_webview_window(LABEL).is_some()
}

pub fn close(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.destroy();
    }
}

/// Ask how it went, for the session that owes an answer.
///
/// One window, ever. A second would mean two WebView2 instances for a question
/// nobody answered the first time, against an idle budget the whole app is
/// built around.
pub fn open(app: &AppHandle, session_id: i64) -> tauri::Result<()> {
    if is_open(app) {
        return Ok(());
    }

    let window = WebviewWindowBuilder::new(
        app,
        LABEL,
        WebviewUrl::App(format!("index.html?window=checkin&session={session_id}").into()),
    )
    .title("Threshold")
    .inner_size(WIDTH, HEIGHT)
    .resizable(false)
    .decorations(false)
    .always_on_top(true)
    // Findable if it ever slips behind something. The ritual hides from the
    // taskbar because it covers the screen; this does not.
    .skip_taskbar(false)
    .visible(false)
    .build()?;

    // Bottom right of the work area, so it sits above the tray it belongs to
    // and never lands on the middle of whatever is being read.
    if let Ok(Some(monitor)) = window.primary_monitor() {
        let scale = monitor.scale_factor();
        let size = monitor.size().to_logical::<f64>(scale);
        let position = monitor.position().to_logical::<f64>(scale);
        let _ = window.set_position(tauri::LogicalPosition::new(
            position.x + size.width - WIDTH - INSET,
            position.y + size.height - HEIGHT - INSET * 2.0,
        ));
    }

    // Shown on the main thread, and never focused.
    //
    // Both lessons are already paid for: a window shown from a worker thread is
    // created and stays invisible, and tao's focus fallback synthesises a
    // left-Alt keypress that opens the Start menu. Not taking focus is why the
    // visible "Not now" control is mandatory rather than a nicety.
    let to_show = window.clone();
    window.app_handle().run_on_main_thread(move || {
        if let Err(err) = to_show.show() {
            crate::log::line(&format!("check-in: could not be shown ({err})"));
        }
    })?;

    Ok(())
}
