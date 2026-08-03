//! A hidden Win32 window whose only job is to hear the operating system tell
//! us the user just came back.
//!
//! Two deliberate choices, both easy to get wrong:
//!
//! 1. This is a real top-level window that is never shown — *not* a
//!    message-only (`HWND_MESSAGE`) window. Broadcast messages such as
//!    `WM_POWERBROADCAST` are delivered only to top-level windows, so a
//!    message-only window would silently never see a wake.
//! 2. It runs on its own thread with its own message pump, because the main
//!    thread already belongs to Tauri's event loop.

use std::sync::OnceLock;

use tauri::AppHandle;
use windows::core::w;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::RemoteDesktop::{
    WTSRegisterSessionNotification, WTSUnRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, PostQuitMessage,
    RegisterClassW, TranslateMessage, MSG, WINDOW_EX_STYLE, WM_DESTROY, WM_POWERBROADCAST,
    WNDCLASSW, WS_OVERLAPPED,
};

use super::{handle_system_event, SystemEvent};

/// Resume from suspend with a user present.
const PBT_APMRESUMESUSPEND: u32 = 0x0007;

/// Resume that the system initiated. On a Modern Standby (S0) machine this is
/// frequently the *only* resume notification delivered, so ignoring it — as
/// this originally did — means never noticing a wake on exactly the hardware
/// most people now own.
///
/// The reason for ignoring it was sound: it also fires when the machine wakes
/// itself for maintenance with nobody there, and prompting an empty room trains
/// the user to dismiss the ritual unread. So it is accepted, but only when a
/// console session is actually attached and unlocked (see `user_is_present`).
const PBT_APMRESUMEAUTOMATIC: u32 = 0x0012;

const WM_WTSSESSION_CHANGE: u32 = 0x02B1;
const WTS_SESSION_LOCK: u32 = 0x7;
const WTS_SESSION_UNLOCK: u32 = 0x8;

static APP: OnceLock<AppHandle> = OnceLock::new();

/// Start listening. Returns immediately; the pump lives on its own thread.
pub fn spawn(app: AppHandle) {
    if APP.set(app).is_err() {
        // Already listening. Registering twice would double every event.
        return;
    }

    std::thread::Builder::new()
        .name("threshold-triggers".into())
        .spawn(|| unsafe { pump() })
        .expect("failed to spawn trigger listener thread");
}

unsafe fn pump() {
    let instance: HINSTANCE = match GetModuleHandleW(None) {
        Ok(module) => module.into(),
        Err(err) => {
            eprintln!("triggers: GetModuleHandleW failed: {err}");
            return;
        }
    };

    let class_name = w!("ThresholdTriggerWindow");
    let class = WNDCLASSW {
        lpfnWndProc: Some(wndproc),
        hInstance: instance,
        lpszClassName: class_name,
        ..Default::default()
    };

    if RegisterClassW(&class) == 0 {
        eprintln!("triggers: RegisterClassW failed");
        return;
    }

    // Never shown: no WS_VISIBLE, zero size, no owner.
    let hwnd = match CreateWindowExW(
        WINDOW_EX_STYLE(0),
        class_name,
        w!("Threshold triggers"),
        WS_OVERLAPPED,
        0,
        0,
        0,
        0,
        None,
        None,
        Some(instance),
        None,
    ) {
        Ok(hwnd) => hwnd,
        Err(err) => {
            eprintln!("triggers: CreateWindowExW failed: {err}");
            return;
        }
    };

    if let Err(err) = WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION) {
        // Wake detection still works without this; only lock/unlock is lost.
        eprintln!("triggers: session notifications unavailable: {err}");
    }

    let mut msg = MSG::default();
    while GetMessageW(&mut msg, None, 0, 0).as_bool() {
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }

    let _ = WTSUnRegisterSessionNotification(hwnd);
}

unsafe extern "system" fn wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_POWERBROADCAST => {
            match wparam.0 as u32 {
                PBT_APMRESUMESUSPEND => dispatch(SystemEvent::Resumed),
                // Only when somebody is actually there to read it.
                PBT_APMRESUMEAUTOMATIC if user_is_present() => {
                    dispatch(SystemEvent::Resumed)
                }
                _ => {}
            }
            // TRUE: the message was handled.
            LRESULT(1)
        }
        WM_WTSSESSION_CHANGE => {
            match wparam.0 as u32 {
                WTS_SESSION_LOCK => dispatch(SystemEvent::Locked),
                WTS_SESSION_UNLOCK => dispatch(SystemEvent::Unlocked),
                _ => {}
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Is there a real, attached console session?
///
/// `WTSGetActiveConsoleSessionId` returns `0xFFFFFFFF` when no session is
/// attached — the machine woke to do maintenance and nobody is looking at it.
/// Distinguishing that from a person opening the lid is the difference between
/// a useful interruption and one that teaches the user to dismiss on reflex.
fn user_is_present() -> bool {
    // SAFETY: no arguments, no allocation; returns a session id or 0xFFFFFFFF.
    let session = unsafe {
        windows::Win32::System::RemoteDesktop::WTSGetActiveConsoleSessionId()
    };
    session != u32::MAX
}

fn dispatch(event: SystemEvent) {
    if let Some(app) = APP.get() {
        handle_system_event(app, event);
    }
}
