//! The fullscreen intention window.
//!
//! Phase 1 only has to prove that a trigger produces exactly one window at the
//! right moment; Phase 2 fills in the ritual itself (spec F1).
//!
//! It covers the primary display and sits above everything, because arriving at
//! a desktop already full of open windows is the failure mode. It is never a
//! jail: the honourable exit stays one click away, and topmost is asserted once
//! rather than defended forever.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::triggers::TriggerKind;

pub const POPUP_LABEL: &str = "popup";

pub fn show(app: &AppHandle, kind: TriggerKind) -> tauri::Result<()> {
    show_with_intent(app, kind, None)
}

/// Open the ritual, optionally with the intention already chosen.
///
/// Whether it *actually reached the screen* is answered later, by the watchdog
/// this spawns. A window can exist, be asked to show, and still not appear —
/// which is what happened when a trigger arrived while the session was locked.
/// Checking that inline is not possible: presentation happens on the event loop,
/// so waiting for it there blocks the thread that performs it.
pub fn show_with_intent(
    app: &AppHandle,
    kind: TriggerKind,
    intent: Option<String>,
) -> tauri::Result<()> {
    // A popup window can survive in a hidden, unusable state - created while the
    // session was locked, or left behind by a ritual that never closed cleanly.
    // Asking it to show does nothing, so it is destroyed and rebuilt rather than
    // reused. Only a window that is genuinely visible gets reused.
    // Never reuse a ritual window. One that exists but is hidden - created
    // behind the lock screen, or orphaned by a ritual that did not close
    // cleanly - can be asked to show all day and will not appear.
    //
    // Tearing one down and building its replacement in the same breath does not
    // work either: destroying a fullscreen window disturbs the display state and
    // the new window is swallowed by it. So the teardown is given a moment to
    // finish first, on a background thread, because waiting here would block the
    // event loop that performs it.
    let stale = ritual_windows(app);
    if !stale.is_empty() {
        for window in stale {
            let _ = window.destroy();
        }
        let app = app.clone();
        std::thread::Builder::new()
            .name("threshold-popup-rebuild".into())
            .spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(400));
                if let Err(err) = build(&app, kind, intent) {
                    eprintln!("popup: could not rebuild the ritual: {err}");
                }
            })
            .expect("failed to spawn the popup rebuild");
        return Ok(());
    }

    build(app, kind, intent)
}

fn build(app: &AppHandle, kind: TriggerKind, intent: Option<String>) -> tauri::Result<()> {

    // `--theme=<id>` previews one of the rotating themes without waiting for
    // its day to come round. Anything unrecognised falls back to the rotation.
    let forced_theme = std::env::args()
        .find_map(|arg| arg.strip_prefix("--theme=").map(str::to_owned))
        .map(|id| format!("&theme={id}"))
        .unwrap_or_default();

    let intent_param = intent
        .filter(|value| !value.trim().is_empty())
        .map(|value| format!("&intent={}", urlencode(&value)))
        .unwrap_or_default();

    let window = WebviewWindowBuilder::new(
        app,
        next_ritual_label(),
        WebviewUrl::App(
            format!(
                "index.html?trigger={}{forced_theme}{intent_param}",
                kind_slug(kind)
            )
            .into(),
        ),
    )
    .title("Threshold")
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .fullscreen(true)
    .visible(false)
    .zoom_hotkeys_enabled(true)
    .build()?;

    window.show()?;

    // Deliberately NOT set_focus().
    //
    // When Windows refuses SetForegroundWindow, tao falls back to synthesising
    // a left-Alt keypress with SendInput to steal the foreground permission
    // (tao window.rs, force_window_active). If the ritual window does not take
    // that focus, the fake keystroke lands on the shell instead — which opened
    // the Start menu every time the machine woke or unlocked.
    //
    // Injecting keystrokes into someone's session is not an acceptable price
    // for focus, and it buys little here: the window is already fullscreen and
    // always-on-top, so it is visible whether or not it holds focus. Keyboard
    // input goes to it once clicked.

    Ok(())
}

pub fn close(app: &AppHandle) -> tauri::Result<()> {
    for window in ritual_windows(app) {
        let _ = window.destroy();
    }
    Ok(())
}

/// Every ritual window, however many have accumulated.
///
/// Labels carry a counter rather than being fixed, because a window destroyed
/// on the event loop keeps its label for a moment afterwards. Matching on the
/// prefix means a replacement never has to wait for its predecessor.
fn ritual_windows(app: &AppHandle) -> Vec<tauri::WebviewWindow> {
    app.webview_windows()
        .into_iter()
        .filter(|(label, _)| label.starts_with(POPUP_LABEL))
        .map(|(_, window)| window)
        .collect()
}

fn next_ritual_label() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    format!("{POPUP_LABEL}-{}", COUNTER.fetch_add(1, Ordering::Relaxed))
}

/// Percent-encode anything that would break out of a query parameter. Task
/// titles are user text and will contain spaces, ampersands and quotes.
fn urlencode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

fn kind_slug(kind: TriggerKind) -> &'static str {
    match kind {
        TriggerKind::Boot => "boot",
        TriggerKind::Wake => "wake",
        TriggerKind::Unlock => "unlock",
        TriggerKind::Manual => "manual",
    }
}
