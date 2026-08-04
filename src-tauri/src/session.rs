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

use std::sync::Mutex;
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

/// How long to wait before asking again, after the helper declines to lift.
///
/// A refusal is not an error: the helper's own monotonic clock still says time
/// is owed, which is exactly the guarantee the commitment lock exists to make.
/// Retrying every poll meant spawning an elevated process twice a minute for as
/// long as the disagreement lasted, so this waits instead.
const LIFT_BACKOFF: [Duration; 4] = [
    Duration::from_secs(60),
    Duration::from_secs(120),
    Duration::from_secs(300),
    Duration::from_secs(600),
];

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

/// One conversation with the helper at a time.
///
/// The request is a single fixed file and the helper task is registered
/// `IgnoreNew`, so two overlapping asks lose one of each: an unblock written
/// while an arm is starting means the helper reads whichever landed last, one
/// `/run` is dropped, and both callers are told about something that never
/// happened. The worst version of that is ending a session and getting a fresh
/// block instead.
static HELPER: Mutex<()> = Mutex::new(());

/// Ask the helper for something, then confirm it actually happened.
///
/// The confirmation is the whole point. Starting the task is not the same as
/// the task succeeding — `schtasks /run` returns as soon as the process is
/// launched, and the helper's own refusal is an exit code nobody can see from
/// here. So every caller reads the world back and reports what is true.
fn ask_helper(
    request: &proto::Request,
    confirmed: impl Fn() -> bool,
    if_unconfirmed: &str,
) -> Result<(), String> {
    // A poisoned mutex here means a previous asker panicked mid-conversation.
    // Refusing to talk to the helper ever again is a worse outcome than
    // continuing, and the next thing we do is read the world back anyway.
    let _serialised = HELPER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    write_request(request)?;
    scheduled_tasks::run_helper().map_err(|err| format!("could not start the helper: {err}"))?;

    if wait_for(confirmed) {
        Ok(())
    } else {
        Err(if_unconfirmed.to_string())
    }
}

/// Why a new block may not be armed, if it may not.
///
/// Arming over a commitment that is still running would overwrite the lock with
/// a fresh `armed_at` and `armed_tick_ms`, silently extending the block and
/// resetting the monotonic baseline the helper uses to refuse an early exit. A
/// commitment you can quietly restart is not a commitment.
///
/// The in-memory debounce usually stops a second ritual reaching this point, but
/// it is an `Instant` that does not survive a restart, so this is the guard that
/// actually holds. Pure, so every case below is a test rather than a claim.
fn rearm_refusal(lock: Option<&proto::Lock>, now: i64) -> Option<String> {
    let left = (lock?.locked_until - now).max(0);
    if left == 0 {
        // Spent, but not yet swept up by the watcher. A new commitment is
        // allowed to replace it.
        return None;
    }
    Some(format!(
        "A commitment is already running, with about {} minutes left. It finishes on its own \
         before another can start.",
        (left + 59) / 60
    ))
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

    if let Some(refusal) = rearm_refusal(active_lock().as_ref(), chrono::Utc::now().timestamp()) {
        return BlockOutcome::failed(refusal);
    }

    let until = chrono::Utc::now().timestamp() + duration_min * 60;
    let request = proto::Request::block(categories.to_vec(), until);

    match ask_helper(
        &request,
        proto::hosts_has_block,
        "The helper did not apply the block. Settings > Repair will re-register it; if that \
         does not help, Windows Defender may be protecting the hosts file.",
    ) {
        Ok(()) => BlockOutcome::ok(),
        Err(reason) => BlockOutcome::failed(reason),
    }
}

/// Ask the helper to lift the block. Used on expiry and at startup for a
/// commitment that ran out while the app was closed.
///
/// Returns `Err` when the hosts file still carries the block afterwards. That
/// distinction was previously thrown away — the confirmation was read and
/// discarded, so this returned `Ok` whenever the helper *started*, whatever it
/// then decided. The caller then announced a lifted block, cleared the tray and
/// the debounce, and left every site blocked with nothing on screen to say so.
pub fn lift() -> Result<(), String> {
    if !scheduled_tasks::helper_registered() {
        return Err("the helper task is not registered".into());
    }
    ask_helper(
        &proto::Request::unblock(),
        || !proto::hosts_has_block(),
        "the helper did not lift the block; its own clock may still owe time",
    )
}

/// Lift the block regardless of the time still owed.
///
/// The helper has always implemented this and nothing in the app ever asked for
/// it: the only construction of `emergency_unblock` in the whole product was in
/// the uninstaller. So the documented way out of a commitment — the one that
/// justifies the lock failing closed on a corrupt file — was to uninstall
/// Threshold. A fail-closed lock is only defensible while its exit exists.
///
/// The helper records every use, neutrally, as a count. That is deliberate: an
/// escape hatch that shames you for taking it still gets taken, and then lied
/// about.
pub fn emergency_unblock() -> Result<(), String> {
    if !scheduled_tasks::helper_registered() {
        return Err("the helper task is not registered".into());
    }
    ask_helper(
        &proto::Request::emergency_unblock(),
        || !proto::hosts_has_block(),
        "the helper did not lift the block",
    )
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
        .spawn(move || {
            // How many times running has the helper declined to lift, and when
            // it is worth asking again. Kept out here rather than retried every
            // poll: each attempt starts an elevated process, and a refusal means
            // the block is still correct, not that anything went wrong.
            let mut refusals: usize = 0;
            let mut retry_at: Option<std::time::Instant> = None;

            loop {
                // A pause outranks a countdown in the tooltip: it is the state
                // that explains why nothing is happening.
                if let Some(until) = crate::pause::paused_until_for(&app) {
                    crate::tray::set_paused(&app, until);
                    std::thread::sleep(EXPIRY_POLL);
                    continue;
                }

                match active_lock() {
                    Some(lock) if seconds_remaining(&lock) == 0 => {
                        let due = match retry_at {
                            None => true,
                            Some(at) => std::time::Instant::now() >= at,
                        };
                        if due {
                            match lift() {
                                Ok(()) => {
                                    println!("session: commitment expired, block lifted");
                                    refusals = 0;
                                    retry_at = None;
                                    crate::triggers::clear_session();
                                    crate::tray::set_status(&app, None);
                                }
                                // The helper refuses if its own monotonic clock
                                // says time is still owed. That is correct, not
                                // an error - so wait, rather than hammering it.
                                Err(err) => {
                                    let wait =
                                        LIFT_BACKOFF[refusals.min(LIFT_BACKOFF.len() - 1)];
                                    refusals += 1;
                                    retry_at = Some(std::time::Instant::now() + wait);
                                    println!(
                                        "session: could not lift yet ({err}); asking again in {}s",
                                        wait.as_secs()
                                    );
                                }
                            }
                        }
                    }
                    Some(lock) => {
                        crate::tray::set_status(&app, Some(seconds_remaining(&lock)));
                    }
                    None => {
                        // The lock is gone, so whatever the disagreement was, it
                        // is over. A later commitment starts from a clean slate.
                        refusals = 0;
                        retry_at = None;
                        crate::tray::set_status(&app, None);
                    }
                }
                std::thread::sleep(EXPIRY_POLL);
            }
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

    fn lock_ending_at(locked_until: i64) -> proto::Lock {
        proto::Lock {
            locked_until,
            armed_at: locked_until - 1_500,
            armed_tick_ms: Some(1_000),
            duration_min: 25,
            categories: vec!["social".into()],
        }
    }

    #[test]
    fn nothing_running_means_nothing_to_refuse() {
        assert!(rearm_refusal(None, 1_800_000_000).is_none());
    }

    #[test]
    fn a_live_commitment_refuses_a_second_one() {
        // Ten minutes left. Without this the second arm would rewrite the lock's
        // monotonic baseline and quietly extend the first commitment.
        let lock = lock_ending_at(1_800_000_600);
        let refusal = rearm_refusal(Some(&lock), 1_800_000_000).expect("should refuse");
        assert!(refusal.contains("10 minutes"), "got: {refusal}");
    }

    #[test]
    fn a_spent_commitment_does_not_block_the_next_one() {
        // Expired but not yet swept up by the watcher, which polls every 30s.
        // Refusing here would make a new session impossible for up to half a
        // minute after the last one ended.
        let lock = lock_ending_at(1_800_000_000);
        assert!(rearm_refusal(Some(&lock), 1_800_000_000).is_none());
        assert!(rearm_refusal(Some(&lock), 1_800_000_045).is_none());
    }

    #[test]
    fn the_lift_backoff_never_indexes_out_of_bounds() {
        // The watcher indexes this by a refusal count that only grows.
        for refusals in 0..50usize {
            let _ = LIFT_BACKOFF[refusals.min(LIFT_BACKOFF.len() - 1)];
        }
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
