//! Arming, watching, and lifting a focus session's block.
//!
//! This is the piece that was missing: the ritual recorded what you committed
//! to and then did nothing about it. Selecting categories wrote a string to a
//! database column nobody read, and every browser stayed open.
//!
//! Two rules here exist because of that failure:
//!
//! * **Never report a block that was not confirmed.** Asking the helper is not
//!   the same as the helper succeeding — it may not be registered, its binary
//!   may be missing, Defender may have refused the write. So after asking, we
//!   read the hosts file back and report what is actually true.
//! * **Something must lift the block.** A commitment that can only be ended by
//!   the emergency exit is a trap, not a commitment.

use std::time::Duration;

use serde::Serialize;
use threshold_protocol as proto;

use crate::triggers::scheduled_tasks;

/// How long to wait for the elevated task to do its work before deciding it
/// has not. Task Scheduler start-up plus a hosts write is well under this.
const CONFIRM_TIMEOUT: Duration = Duration::from_secs(12);
const CONFIRM_POLL: Duration = Duration::from_millis(400);

/// How often to check whether a running commitment has expired.
const EXPIRY_POLL: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockOutcome {
    /// True only when the hosts file was read back and actually carries the block.
    pub blocked: bool,
    /// Why not, in language that belongs on screen rather than in a log.
    pub reason: Option<String>,
}

impl BlockOutcome {
    fn ok() -> Self {
        Self {
            blocked: true,
            reason: None,
        }
    }

    fn failed(reason: impl Into<String>) -> Self {
        Self {
            blocked: false,
            reason: Some(reason.into()),
        }
    }
}

fn write_request(request: &proto::Request) -> Result<(), String> {
    let dir = proto::program_data();
    std::fs::create_dir_all(&dir)
        .map_err(|err| format!("could not create {}: {err}", dir.display()))?;
    let body = serde_json::to_string(request)
        .map_err(|err| format!("could not encode the request: {err}"))?;
    std::fs::write(proto::request_path(), body)
        .map_err(|err| format!("could not write the request: {err}"))
}

/// Ask the helper to block, then confirm it happened.
pub fn arm(categories: &[String], duration_min: i64) -> BlockOutcome {
    if categories.is_empty() || duration_min <= 0 {
        return BlockOutcome::failed("nothing was selected to block");
    }

    if !scheduled_tasks::helper_registered() {
        return BlockOutcome::failed(
            "Threshold cannot change the hosts file yet. Open Settings and choose Repair to \
             finish setup - it needs administrator rights once.",
        );
    }

    let until = chrono::Utc::now().timestamp() + duration_min * 60;
    let request = proto::Request::block(categories.to_vec(), until);

    if let Err(err) = write_request(&request) {
        return BlockOutcome::failed(err);
    }
    if let Err(err) = scheduled_tasks::run_helper() {
        return BlockOutcome::failed(format!("could not start the helper: {err}"));
    }

    if wait_for(proto::hosts_has_block) {
        BlockOutcome::ok()
    } else {
        BlockOutcome::failed(
            "The helper did not apply the block. Settings > Repair will re-register it; if that \
             does not help, Windows Defender may be protecting the hosts file.",
        )
    }
}

/// Ask the helper to lift the block. Used on expiry and at startup for a
/// commitment that ran out while the app was closed.
pub fn lift() -> Result<(), String> {
    if !scheduled_tasks::helper_registered() {
        return Err("the helper task is not registered".into());
    }
    write_request(&proto::Request::unblock())?;
    scheduled_tasks::run_helper()?;
    let _ = wait_for(|| !proto::hosts_has_block());
    Ok(())
}

fn wait_for(condition: impl Fn() -> bool) -> bool {
    let deadline = std::time::Instant::now() + CONFIRM_TIMEOUT;
    while std::time::Instant::now() < deadline {
        if condition() {
            return true;
        }
        std::thread::sleep(CONFIRM_POLL);
    }
    condition()
}

/// The lock as it stands on disk, if a commitment is running.
pub fn active_lock() -> Option<proto::Lock> {
    let raw = std::fs::read_to_string(proto::lock_path()).ok()?;
    proto::parse_json::<proto::Lock>(&raw).ok()
}

/// Seconds left on the current commitment, by wall clock.
///
/// The helper owns the authoritative decision (it also consults a monotonic
/// clock); this is only for deciding when to ask, and for showing a countdown.
pub fn seconds_remaining(lock: &proto::Lock) -> i64 {
    (lock.locked_until - chrono::Utc::now().timestamp()).max(0)
}

/// Watch for the commitment running out, including one that expired while the
/// app was closed, and keep the tray showing how long is left.
///
/// A background thread rather than a timer tied to the session that armed it:
/// the app is quit, restarted and slept constantly, and a block whose only
/// release lives in memory would survive all three.
pub fn spawn_expiry_watcher(app: tauri::AppHandle) {
    std::thread::Builder::new()
        .name("threshold-expiry".into())
        .spawn(move || loop {
            // A pause outranks a countdown in the tooltip: it is the state that
            // explains why nothing is happening.
            if let Some(until) = crate::pause::paused_until_for(&app) {
                crate::tray::set_paused(&app, until);
                std::thread::sleep(EXPIRY_POLL);
                continue;
            }

            match active_lock() {
                Some(lock) if seconds_remaining(&lock) == 0 => {
                    match lift() {
                        Ok(()) => {
                            println!("session: commitment expired, block lifted");
                            crate::triggers::clear_session();
                            crate::tray::set_status(&app, None);
                        }
                        // The helper refuses if its own monotonic clock says
                        // time is still owed. That is correct, not an error.
                        Err(err) => println!("session: could not lift yet ({err})"),
                    }
                }
                Some(lock) => {
                    crate::tray::set_status(&app, Some(seconds_remaining(&lock)));
                }
                None => crate::tray::set_status(&app, None),
            }
            std::thread::sleep(EXPIRY_POLL);
        })
        .expect("failed to spawn the expiry watcher");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lock(until_offset: i64) -> proto::Lock {
        proto::Lock {
            locked_until: chrono::Utc::now().timestamp() + until_offset,
            armed_at: chrono::Utc::now().timestamp(),
            armed_tick_ms: Some(1),
            duration_min: 25,
            categories: vec!["social".into()],
        }
    }

    #[test]
    fn a_running_commitment_has_time_left() {
        assert!(seconds_remaining(&lock(600)) > 0);
    }

    #[test]
    fn an_elapsed_commitment_reports_zero_not_a_negative() {
        assert_eq!(seconds_remaining(&lock(-600)), 0);
    }

    #[test]
    fn arming_nothing_is_refused_rather_than_reported_as_blocked() {
        let outcome = arm(&[], 25);
        assert!(!outcome.blocked);
        assert!(outcome.reason.is_some());
    }

    #[test]
    fn arming_with_no_duration_is_refused() {
        let outcome = arm(&["social".to_string()], 0);
        assert!(!outcome.blocked);
    }

    #[test]
    fn a_request_round_trips_through_the_shared_contract() {
        let request = proto::Request::block(vec!["social".into()], 1_800_000_000);
        let encoded = serde_json::to_string(&request).unwrap();
        let decoded: proto::Request = proto::parse_json(&encoded).unwrap();
        assert_eq!(decoded.action, proto::Action::Block);
        assert_eq!(decoded.until, Some(1_800_000_000));
        assert!(!decoded.dry_run);
    }
}
