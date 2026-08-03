//! Phase 1 — the moments Threshold exists to catch.
//!
//! A hidden Win32 message window owns all three:
//!   * logon      — app autostart, popup on launch
//!   * wake       — WM_POWERBROADCAST / PBT_APMRESUMESUSPEND only
//!                  (PBT_APMRESUMEAUTOMATIC means no user is present)
//!   * unlock     — WTSRegisterSessionNotification → WTS_SESSION_UNLOCK
//!
//! Modern Standby laptops deliver resume events unreliably, so wake and unlock
//! overlap on purpose and are deduped: never twice within 15 minutes, never
//! during an active session, and for unlock only past the lock threshold.
//!
//! Planned split: message_window.rs, debounce.rs, scheduled_tasks.rs.
