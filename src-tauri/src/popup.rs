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
    // Already open: bring it forward rather than stacking a second ritual.
    if let Some(existing) = app.get_webview_window(POPUP_LABEL) {
        existing.show()?;
        existing.set_focus()?;
        return Ok(());
    }

    let window = WebviewWindowBuilder::new(
        app,
        POPUP_LABEL,
        WebviewUrl::App(format!("index.html?trigger={}", kind_slug(kind)).into()),
    )
    .title("Threshold")
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .fullscreen(true)
    .visible(false)
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

fn kind_slug(kind: TriggerKind) -> &'static str {
    match kind {
        TriggerKind::Boot => "boot",
        TriggerKind::Wake => "wake",
        TriggerKind::Unlock => "unlock",
        TriggerKind::Manual => "manual",
    }
}
