//! The window that answers a blocked site.
//!
//! In the middle of the screen, briefly, and gone on its own. Not the ritual:
//! the ritual takes the whole screen because it stands at a threshold where
//! nothing is decided yet. Here everything is already decided — you committed,
//! and the block held. A fullscreen takeover at that moment would be a
//! scolding, and scolding is what makes people defeat a tool rather than use
//! it. But a card in the corner was too easy to never see, and a line you chose
//! for exactly this moment deserves to be met — so it stands where the eye
//! already is, over the browser that just failed, and steps aside by itself.
//!
//! It carries one thing: a line the user chose. Not a lecture, not a counter, no
//! mention of what was reached for — naming the thing you are avoiding makes it
//! more available, not less, which is the same reason the session banner names
//! the task and never the blocklist.
//!
//! Label rules are the check-in's, for the same reasons: it must not begin with
//! `popup` (`popup::close` destroys everything that does) and it must be in the
//! `CloseRequested` exception in `lib.rs`, or it is hidden rather than destroyed
//! and can never be shown again.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub const LABEL: &str = "blocked";

/// Logical size. Room for a long line in a display serif at a size that can be
/// read from where you sit, plus a margin the card's shadow and glow can bleed
/// into - the window is transparent, so the card is the shape you see.
const WIDTH: f64 = 720.0;
const HEIGHT: f64 = 400.0;

/// How long it stays before removing itself.
///
/// It is answering a moment, not opening a conversation, and it takes no focus —
/// so nothing would ever close it otherwise. Long enough to read twice. Sent to
/// the page as well, so the drain line and the exit fade end when the window
/// does rather than a second before or after.
const LINGER: std::time::Duration = std::time::Duration::from_secs(12);

pub fn close(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.destroy();
    }
}

/// Show the wall, from whatever thread noticed the attempt.
///
/// The building happens on the main thread, and this returns without waiting.
/// Both halves matter and both were learned the hard way: creating a window from
/// a worker thread blocks that thread on the event loop, and the caller here is
/// a socket accept loop — so a wall raised inline froze the UI *and* stopped the
/// sink accepting, leaving browser connections open and unanswered. Errors are
/// logged, never surfaced: this is a courtesy on top of a block that has already
/// done its job.
pub fn show(app: &AppHandle, host: Option<&str>) {
    let owner = app.clone();
    let host = host.map(str::to_owned);
    let queued = app.run_on_main_thread(move || {
        if let Err(err) = open(&owner, host.as_deref()) {
            crate::log::line(&format!("wall: could not be shown ({err})"));
        }
    });
    if let Err(err) = queued {
        crate::log::line(&format!("wall: could not reach the main thread ({err})"));
    }
}

/// Where the card goes: the optical centre of the screen the cursor is on.
///
/// The cursor's monitor rather than the primary one, because the browser that
/// just failed is where the cursor is. A little above true centre - 42% of the
/// free height rather than 50% - which is where the eye expects a centred
/// thing to be; dead centre reads as slightly low.
fn place(window: &tauri::WebviewWindow) -> Option<tauri::LogicalPosition<f64>> {
    let app = window.app_handle();
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|cursor| app.monitor_from_point(cursor.x, cursor.y).ok().flatten())
        .or_else(|| window.primary_monitor().ok().flatten())?;
    let scale = monitor.scale_factor();
    let size = monitor.size().to_logical::<f64>(scale);
    let origin = monitor.position().to_logical::<f64>(scale);
    Some(tauri::LogicalPosition::new(
        origin.x + (size.width - WIDTH) / 2.0,
        origin.y + (size.height - HEIGHT) * 0.42,
    ))
}

fn open(app: &AppHandle, host: Option<&str>) -> tauri::Result<()> {
    // A ritual is a fullscreen takeover; putting this on top of one would be two
    // windows arguing. The ritual is also the better place to be.
    if !crate::popup::ritual_windows(app).is_empty() {
        return Ok(());
    }
    if app.get_webview_window(LABEL).is_some() {
        return Ok(());
    }

    let query = host
        .map(|host| format!("&host={}", crate::popup::urlencode(host)))
        .unwrap_or_default();

    let window = WebviewWindowBuilder::new(
        app,
        LABEL,
        WebviewUrl::App(
            format!(
                "index.html?window=blocked&linger={}{query}",
                LINGER.as_millis()
            )
            .into(),
        ),
    )
    .title("Threshold")
    .theme(Some(tauri::Theme::Dark))
    .inner_size(WIDTH, HEIGHT)
    .resizable(false)
    .decorations(false)
    // The card draws its own shape; the window is only the space it floats in.
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .visible(false)
    // The chime plays with nobody having clicked - Chromium would otherwise
    // hold it for a gesture that never comes. The feature list is wry's own
    // default, restated because setting any argument replaces it wholesale.
    .additional_browser_args(
        "--autoplay-policy=no-user-gesture-required \
         --disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection",
    )
    .build()?;

    if let Some(position) = place(&window) {
        let _ = window.set_position(position);
    }

    // Already on the main thread — `show` scheduled us here — so this is a plain
    // call rather than another hop. Never focused: tao's focus fallback
    // synthesises a left-Alt keypress that opens the Start menu, and stealing
    // focus from whatever you are meant to be doing would rather defeat the
    // point of the thing.
    if let Err(err) = window.show() {
        crate::log::line(&format!("wall: could not be shown ({err})"));
    }

    let owner = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(LINGER);
        let closing = owner.clone();
        let _ = owner.run_on_main_thread(move || close(&closing));
    });

    Ok(())
}
