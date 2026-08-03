//! The commitment lock.
//!
//! `lock.json` lives in ProgramData, which is ACL'd so a limited user cannot
//! edit it. That is the whole mechanism: the UI cannot lift a block early, and
//! neither can you by opening a file - only the emergency path, which is
//! deliberately costly and always recorded.
//!
//! # Why two clocks
//!
//! Wall-clock time alone cannot survive tampering. Wind the system clock back a
//! year and any wall-clock check either releases the block immediately or holds
//! it for a year - there is no third option, because elapsed real time is
//! simply not knowable from a clock that moved.
//!
//! So the arm time is recorded twice: once in wall-clock seconds, and once as
//! milliseconds since boot, which no clock change can affect. Within one boot
//! the monotonic reading decides and tampering does nothing. Across a reboot
//! the tick counter resets, so wall-clock takes over - a real limitation, and
//! the reason the wall-clock path stays conservative and holds when the numbers
//! look impossible rather than releasing.

use serde::{Deserialize, Serialize};

pub const LOCK_FILE: &str = "lock.json";
pub const DRIFT_FILE: &str = "drift.json";

/// How far back the clock may move before we stop believing it.
const CLOCK_TOLERANCE_SECS: i64 = 300;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lock {
    /// Unix seconds when the block may be lifted.
    pub locked_until: i64,
    /// Unix seconds when it was armed.
    pub armed_at: i64,
    /// Milliseconds since boot when it was armed. Immune to clock changes.
    /// Optional so a lock written before this existed - or by a tool that emits
    /// `null` - still parses rather than being treated as corrupt.
    #[serde(default)]
    pub armed_tick_ms: Option<u64>,
    /// Minutes committed to.
    pub duration_min: i64,
    pub categories: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    NotLocked,
    Expired,
    Held { seconds_remaining: i64 },
    /// A lock file exists but cannot be understood. Treated as held.
    Unreadable,
}

/// What was found on disk. The distinction matters: "no lock file" and "a lock
/// file I cannot read" must not be handled the same way.
#[derive(Debug)]
pub enum Stored {
    Absent,
    Present(Lock),
    /// Present but unparseable - corrupt, truncated, or edited.
    Corrupt,
}

/// Milliseconds since boot. Unaffected by changes to the system clock.
#[cfg(windows)]
pub fn tick_ms() -> u64 {
    unsafe { windows::Win32::System::SystemInformation::GetTickCount64() }
}

#[cfg(not(windows))]
pub fn tick_ms() -> u64 {
    0
}

/// Decide whether a block may be lifted.
///
/// `now` is wall-clock seconds; `tick` is milliseconds since boot.
///
/// A corrupt lock file fails **closed**. This is a lock: the safe direction
/// when its state cannot be read is to keep holding, otherwise damaging the
/// file becomes a way to walk out early. The emergency path still works, so
/// nobody is ever actually stuck.
pub fn evaluate(stored: &Stored, now: i64, tick: u64) -> Verdict {
    let lock = match stored {
        Stored::Absent => return Verdict::NotLocked,
        Stored::Corrupt => return Verdict::Unreadable,
        Stored::Present(lock) => lock,
    };

    let duration_secs = lock.duration_min.max(0) * 60;

    // Same boot: the tick counter is authoritative and tamper-proof.
    if let Some(armed_tick) = lock.armed_tick_ms.filter(|t| *t > 0 && tick >= *t) {
        let elapsed = ((tick - armed_tick) / 1000) as i64;
        return if elapsed >= duration_secs {
            Verdict::Expired
        } else {
            Verdict::Held {
                seconds_remaining: duration_secs - elapsed,
            }
        };
    }

    // Rebooted (or no tick recorded): fall back to wall-clock.
    let clock_looks_sane = now >= lock.armed_at - CLOCK_TOLERANCE_SECS;

    if clock_looks_sane {
        if now >= lock.locked_until {
            Verdict::Expired
        } else {
            Verdict::Held {
                seconds_remaining: lock.locked_until - now,
            }
        }
    } else {
        // The clock moved backwards across a reboot, so elapsed time is
        // unknowable. Hold rather than hand out a free unblock to anyone who
        // changes their clock; the emergency path still exists.
        Verdict::Held {
            seconds_remaining: duration_secs,
        }
    }
}

pub fn parse(raw: &str) -> Result<Lock, String> {
    serde_json::from_str(raw.trim_start_matches('\u{feff}'))
        .map_err(|err| format!("lock file is unreadable: {err}"))
}

/// Read the lock, distinguishing "absent" from "present but unreadable".
pub fn load(path: &std::path::Path) -> Stored {
    match std::fs::read_to_string(path) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Stored::Absent,
        Err(_) => Stored::Corrupt,
        Ok(raw) if raw.trim().is_empty() => Stored::Corrupt,
        Ok(raw) => match parse(&raw) {
            Ok(lock) => Stored::Present(lock),
            Err(_) => Stored::Corrupt,
        },
    }
}

pub fn render(lock: &Lock) -> String {
    serde_json::to_string_pretty(lock).unwrap_or_default()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftEvent {
    pub ts: i64,
    pub seconds_early: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: u64 = 60_000;

    fn lock(armed_at: i64, armed_tick_ms: u64, duration_min: i64) -> Stored {
        Stored::Present(Lock {
            locked_until: armed_at + duration_min * 60,
            armed_at,
            armed_tick_ms: Some(armed_tick_ms),
            duration_min,
            categories: vec!["social".into()],
        })
    }

    #[test]
    fn no_lock_means_nothing_to_hold() {
        assert_eq!(evaluate(&Stored::Absent, 1_000, 0), Verdict::NotLocked);
    }

    #[test]
    fn a_corrupt_lock_holds_rather_than_releasing() {
        // Damaging the file must never be a way out; that is the whole point.
        assert_eq!(evaluate(&Stored::Corrupt, 1_000, 0), Verdict::Unreadable);
    }

    #[test]
    fn a_lock_with_a_null_tick_is_readable_not_corrupt() {
        let raw = r#"{"locked_until":2000,"armed_at":1000,"armed_tick_ms":null,
                      "duration_min":25,"categories":["social"]}"#;
        let parsed = parse(raw).expect("null tick should parse");
        assert_eq!(parsed.armed_tick_ms, None);
    }

    #[test]
    fn refuses_while_the_commitment_is_still_running() {
        let l = lock(1_000, 10 * MIN, 25);
        match evaluate(&l, 1_600, 20 * MIN) {
            Verdict::Held { seconds_remaining } => assert_eq!(seconds_remaining, 900),
            other => panic!("expected Held, got {other:?}"),
        }
    }

    #[test]
    fn releases_once_the_committed_time_has_actually_elapsed() {
        let l = lock(1_000, 10 * MIN, 25);
        assert_eq!(evaluate(&l, 2_500, 35 * MIN), Verdict::Expired);
    }

    #[test]
    fn the_monotonic_clock_ignores_a_tampered_wall_clock() {
        let l = lock(1_000_000, 10 * MIN, 50);
        // Wall clock yanked back a year, but only 5 real minutes have passed.
        let verdict = evaluate(&l, 1_000_000 - 31_536_000, 15 * MIN);
        match verdict {
            Verdict::Held { seconds_remaining } => assert_eq!(seconds_remaining, 45 * 60),
            other => panic!("expected Held, got {other:?}"),
        }
    }

    #[test]
    fn a_tampered_clock_cannot_extend_the_block_either() {
        let l = lock(1_000_000, 10 * MIN, 50);
        // Wall clock is nonsense, but the tick counter says the time is served.
        let verdict = evaluate(&l, 1_000_000 - 31_536_000, 61 * MIN);
        assert_eq!(verdict, Verdict::Expired);
    }

    #[test]
    fn after_a_reboot_wall_clock_takes_over() {
        // Tick counter reset below the armed value: a reboot happened.
        let l = lock(1_000, 500 * MIN, 25);
        assert_eq!(evaluate(&l, 1_000 + 1_500, 2 * MIN), Verdict::Expired);
    }

    #[test]
    fn a_reboot_mid_session_still_holds_the_remaining_time() {
        let l = lock(1_000, 500 * MIN, 25);
        match evaluate(&l, 1_600, 2 * MIN) {
            Verdict::Held { seconds_remaining } => assert_eq!(seconds_remaining, 900),
            other => panic!("expected Held, got {other:?}"),
        }
    }

    #[test]
    fn a_clock_wound_back_across_a_reboot_holds_rather_than_releasing() {
        let l = lock(1_000_000, 500 * MIN, 50);
        let verdict = evaluate(&l, 1_000_000 - 31_536_000, 2 * MIN);
        assert!(matches!(verdict, Verdict::Held { .. }), "got {verdict:?}");
    }

    #[test]
    fn small_ntp_drift_does_not_change_the_verdict() {
        let l = Stored::Present(Lock {
            locked_until: 1_000_000 + 1_500,
            armed_at: 1_000_000,
            armed_tick_ms: None,
            duration_min: 25,
            categories: vec![],
        });
        match evaluate(&l, 1_000_000 - 60, 0) {
            Verdict::Held { .. } => {}
            other => panic!("expected Held, got {other:?}"),
        }
    }

    #[test]
    fn round_trips_through_json() {
        let Stored::Present(l) = lock(1_000, 10 * MIN, 25) else {
            panic!("constructed a present lock")
        };
        let parsed = parse(&render(&l)).expect("round trip");
        assert_eq!(parsed.locked_until, l.locked_until);
        assert_eq!(parsed.armed_tick_ms, Some(10 * MIN));
    }

    #[test]
    fn a_lock_written_before_ticks_existed_still_parses() {
        let raw = r#"{"locked_until":2000,"armed_at":1000,"duration_min":25,"categories":[]}"#;
        let parsed = parse(raw).expect("older lock file");
        assert_eq!(parsed.armed_tick_ms, None);
    }
}
