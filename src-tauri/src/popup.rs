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
pub fn show_with_intent(
    app: &AppHandle,
    kind: TriggerKind,
    intent: Option<String>,
) -> tauri::Result<()> {
    // Already open: bring it forward rather than stacking a second ritual.
    if let Some(existing) = app.get_webview_window(POPUP_LABEL) {
        existing.show()?;
        existing.set_focus()?;
        return Ok(());
    }

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
        POPUP_LABEL,
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

    // At logon and wake there is no competing foreground window, so this
    // succeeds; at other times Windows may refuse and that is fine.
    let _ = window.set_focus();

    Ok(())
}

pub fn close(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(POPUP_LABEL) {
        window.close()?;
    }
    Ok(())
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
