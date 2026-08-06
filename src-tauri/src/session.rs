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

    pub(crate) fn failed(reason: impl Into<String>) -> Self {
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

/// Tell every window a session has begun.
///
/// The first events in this codebase. They are notifications, never the record:
/// the dashboard re-reads `session_status` on mount and whenever it becomes
/// visible again, so an event emitted while no window existed costs nothing.
pub fn announce_started(app: &tauri::AppHandle, session_id: i64) {
    use tauri::Emitter;
    let _ = app.emit("session-started", session_id);
}

/// Tell every window a session is over, and why.
pub fn announce_ended(app: &tauri::AppHandle, session_id: i64, reason: &str) {
    use tauri::Emitter;
    let _ = app.emit(
        "session-ended",
        serde_json::json!({ "sessionId": session_id, "reason": reason }),
    );
}

/// How long after a session ends it is still worth asking how it went.
///
/// Ten minutes. Past that the answer is a reconstruction rather than a report,
/// and a guessed answer scored against a real prediction is worse than no
/// answer at all - it is the one input this whole loop exists to collect.
pub const CHECKIN_GRACE: i64 = 10 * 60;

/// What the world looks like, given a session record and a block lock.
///
/// The two are separate systems with separate clocks and separate owners, so
/// they can disagree. Every way they can is enumerated here rather than
/// discovered later. Pure, like the helper's `lock::evaluate`, so each case is
/// a test rather than a claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reconciled {
    /// Nothing running, nothing owed.
    Idle,
    /// A session is counting down. `block_seconds` is what the lock says, when
    /// there is one - it may differ from the session's own remainder.
    Running {
        seconds_remaining: i64,
        block_seconds: Option<i64>,
    },
    /// The session is over and nobody has been asked yet.
    CheckinDue {
        late_by: i64,
        block_seconds: Option<i64>,
    },
    /// Over so long ago that asking would be asking someone to guess.
    TooLate,
    /// A block with no session behind it: upgraded mid-commitment, or the
    /// database was replaced. Nothing is invented to fill the gap.
    BlockOnly { block_seconds: i64 },
}

pub fn reconcile(
    session: Option<&crate::db::sessions::SessionRow>,
    lock: Option<&proto::Lock>,
    now: i64,
    grace: i64,
) -> Reconciled {
    use crate::db::sessions::State;

    let block_seconds = lock
        .map(|lock| (lock.locked_until - now).max(0))
        .filter(|left| *left > 0);

    let Some(session) = session else {
        return match block_seconds {
            Some(block_seconds) => Reconciled::BlockOnly { block_seconds },
            None => Reconciled::Idle,
        };
    };

    let left = session.ends_ts - now;

    match session.state {
        State::Running if left > 0 => Reconciled::Running {
            seconds_remaining: left,
            block_seconds,
        },
        // Over. Whether the block agrees is a separate question with a separate
        // answer: the session ends on time even when the helper will not yet
        // lift, because the commitment is to the task, not to the hosts file.
        State::Running | State::AwaitingCheckin => {
            let late_by = (-left).max(0);
            if late_by <= grace {
                Reconciled::CheckinDue {
                    late_by,
                    block_seconds,
                }
            } else {
                Reconciled::TooLate
            }
        }
        // Already answered or already closed.
        _ => match block_seconds {
            Some(block_seconds) => Reconciled::BlockOnly { block_seconds },
            None => Reconciled::Idle,
        },
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
pub fn arm(categories: &[String], custom_hosts: &[String], duration_min: i64) -> BlockOutcome {
    if (categories.is_empty() && custom_hosts.is_empty()) || duration_min <= 0 {
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

    let now = chrono::Utc::now().timestamp();
    let until = now + duration_min * 60;
    let request = proto::Request::block(categories.to_vec(), custom_hosts.to_vec(), until, now);

    // Confirming that *a* block exists is not the same as confirming that the
    // sites you named are in it. A helper too old to understand a field the app
    // has started sending accepts the request and blocks only the part it
    // recognises — `#[serde(default)]` guarantees it — and the marker check would
    // call that a success. So wait for the block, then check the contents.
    let wanted = custom_hosts.to_vec();
    match ask_helper(
        &request,
        || proto::hosts_has_block() && proto::hosts_missing(&wanted).is_empty(),
        "The helper did not apply the block. Settings > Repair will re-register it; if that \
         does not help, Windows Defender may be protecting the hosts file.",
    ) {
        Ok(()) => BlockOutcome::ok(),
        Err(reason) => {
            // Distinguish "nothing happened" from "your own sites were dropped",
            // because the second one looks like success from every other angle.
            let missing = proto::hosts_missing(custom_hosts);
            if proto::hosts_has_block() && !missing.is_empty() {
                return BlockOutcome::failed(format!(
                    "The block is on, but these sites were not included: {}. The blocking \
                     helper is older than this version of Threshold — Settings > Repair will \
                     replace it.",
                    missing.join(", ")
                ));
            }
            BlockOutcome::failed(reason)
        }
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
        released,
        "the helper did not lift the block; its own clock may still owe time",
    )
}

/// Is the block genuinely gone — hosts file *and* browser policy?
///
/// Deliberately not `!hosts_has_block()`. That was true when the hosts file
/// could not be read at all, so a momentary read failure satisfied the very
/// first poll and the app announced freedom it had not confirmed.
///
/// Browser policy is checked too, via the backup file the helper deletes only
/// after reading the registry back. It is the layer that actually blocks a
/// service worker, so leaving it in force while saying "the sites are open
/// again" would reproduce the original complaint exactly.
fn released() -> bool {
    proto::hosts_confirmed_clear() && !proto::policy_applied()
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
        released,
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

/// Close what finished while the app was shut, and re-arm what did not.
///
/// Two distinct jobs, in this order. **Reap**: nothing closes a session row when
/// the process is killed, so an open row is not evidence that anything is
/// running — only `ends_ts` is. **Rehydrate**: at most one row can still be
/// live, and it is re-armed for the time it has left rather than for its full
/// duration, or the first launch after a week away would suppress the boot
/// ritual for a session that ended days ago.
///
/// Must run after `triggers::init` (which replaces the state wholesale) and
/// before the boot trigger fires.
pub fn reconcile_on_startup(app: &tauri::AppHandle) {
    use crate::db::{sessions, Db};
    use tauri::Manager;

    let now = chrono::Utc::now().timestamp();
    let lock = active_lock();

    let Some(state) = app.try_state::<Db>() else {
        return;
    };
    let Ok(conn) = state.0.lock() else {
        return;
    };

    let open = sessions::open_sessions(&conn).unwrap_or_default();

    // Everything already over gets closed here, whatever the row still claims.
    let mut live = None;
    for row in open {
        match reconcile(Some(&row), lock.as_ref(), now, CHECKIN_GRACE) {
            Reconciled::Running { .. } => live = Some(row),
            Reconciled::CheckinDue { .. } => {
                let _ = sessions::mark(&conn, row.id, sessions::State::AwaitingCheckin, Some(row.ends_ts));
                crate::log::line("session: a check-in is owed from before this launch");
            }
            _ => {
                let _ = sessions::mark(&conn, row.id, sessions::State::Lapsed, Some(row.ends_ts));
                crate::log::line("session: closed one that ran out while the app was shut");
            }
        }
    }

    match (live, lock.as_ref()) {
        (Some(row), _) => {
            crate::triggers::note_session_until(row.ends_ts, row.enforced);
            crate::tray::set_status(app, Some((row.ends_ts - now).max(0)));
            crate::log::line("session: resumed one that was still running");
        }
        // A block with no session behind it: upgraded mid-commitment, or the
        // database was replaced. Nothing is invented, but the debounce still
        // learns about it - which is the long-standing defect where restarting
        // mid-block re-prompted on top of a live commitment.
        (None, Some(lock)) if seconds_remaining(lock) > 0 => {
            crate::triggers::note_session_until(lock.locked_until, true);
            crate::tray::set_status(app, Some(seconds_remaining(lock)));
        }
        _ => {}
    }
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
                //
                // It does not, however, skip the session work below. A pause
                // silences future rituals; swallowing the closing question for
                // a session the user themself started would lose the answer
                // entirely, and a session cannot even begin while paused.
                let paused = crate::pause::paused_until_for(&app);
                if let Some(until) = paused {
                    crate::tray::set_paused(&app, until);
                }

                let session_seconds = advance_sessions(&app);
                offer_checkin(&app);

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
                        if paused.is_none() {
                            // The session's own remainder wins when it is the
                            // longer of the two: a block that outlives its
                            // session, or the reverse, are both real and the
                            // tooltip should not claim the shorter one is all
                            // that is left.
                            let left = seconds_remaining(&lock).max(session_seconds.unwrap_or(0));
                            crate::tray::set_status(&app, Some(left));
                        }
                    }
                    None => {
                        // The lock is gone, so whatever the disagreement was, it
                        // is over. A later commitment starts from a clean slate.
                        refusals = 0;
                        retry_at = None;
                        if paused.is_none() {
                            crate::tray::set_status(&app, session_seconds);
                        }
                    }
                }

                // Sleep only until the next thing that can happen, so a session
                // ending is noticed in about a second rather than up to thirty -
                // and so the 95% of the time nothing is running costs nothing.
                let wait = session_seconds
                    .into_iter()
                    .chain(active_lock().map(|lock| seconds_remaining(&lock)))
                    .filter(|left| *left > 0)
                    .min()
                    .map(|left| Duration::from_secs(left.clamp(1, 30) as u64))
                    .unwrap_or(EXPIRY_POLL);
                std::thread::sleep(wait);
            }
        })
        .expect("failed to spawn the expiry watcher");
}

/// Put the closing question on screen, when there is somewhere to put it.
///
/// Four things have to be true, and each one is a bug that has already happened
/// somewhere in this app:
///
/// * within the grace window — past that the answer is a reconstruction, and a
///   guessed answer scored against a real prediction is worse than none;
/// * the screen is unlocked — a window built behind the lock screen is never
///   seen, which is the exact failure the wake ritual was fixed for;
/// * no ritual is open — two windows fighting for the same moment, neither
///   holding focus, is worse than either alone. The question waits; the ritual
///   is the more time-critical of the two;
/// * it is not already open — one window, ever.
fn offer_checkin(app: &tauri::AppHandle) {
    use crate::db::{sessions, Db};
    use tauri::Manager;

    if crate::checkin::is_open(app) || !crate::popup::ritual_windows(app).is_empty() {
        return;
    }
    if crate::triggers::workstation_locked() {
        return;
    }

    let now = chrono::Utc::now().timestamp();
    let Some(state) = app.try_state::<Db>() else {
        return;
    };
    let Ok(conn) = state.0.lock() else {
        return;
    };
    let Ok(Some(row)) = sessions::awaiting(&conn) else {
        return;
    };

    let ended = row.ended_ts.unwrap_or(row.ends_ts);
    if now - ended > CHECKIN_GRACE {
        let _ = sessions::mark(&conn, row.id, sessions::State::Lapsed, None);
        drop(conn);
        crate::log::line("session: the check-in window passed; not asking after the fact");
        return;
    }

    let id = row.id;
    drop(conn);
    if let Err(err) = crate::checkin::open(app, id) {
        crate::log::line(&format!("check-in: could not open ({err})"));
    }
}

/// Move any session past its deadline, and report what is still counting down.
///
/// Deliberately independent of whether the block could be lifted. The session
/// is a commitment to a task; the block is enforcement that may outlive it. If
/// the helper's clock still owes time the sites stay quiet, and the check-in
/// says so - but the session itself is over on schedule.
fn advance_sessions(app: &tauri::AppHandle) -> Option<i64> {
    use crate::db::{sessions, Db};
    use tauri::Manager;

    let now = chrono::Utc::now().timestamp();
    let state = app.try_state::<Db>()?;
    let conn = state.0.lock().ok()?;

    let live = sessions::live(&conn).ok().flatten()?;
    match reconcile(Some(&live), active_lock().as_ref(), now, CHECKIN_GRACE) {
        Reconciled::Running {
            seconds_remaining, ..
        } => Some(seconds_remaining),
        Reconciled::CheckinDue { .. } => {
            let _ = sessions::mark(
                &conn,
                live.id,
                sessions::State::AwaitingCheckin,
                Some(live.ends_ts),
            );
            crate::triggers::clear_session();
            drop(conn);
            crate::log::line("session: time is up, the check-in is owed");
            announce_ended(app, live.id, "expired");
            None
        }
        _ => {
            let _ = sessions::mark(&conn, live.id, sessions::State::Lapsed, Some(live.ends_ts));
            crate::triggers::clear_session();
            drop(conn);
            crate::log::line("session: ran out with nobody around to ask");
            announce_ended(app, live.id, "lapsed");
            None
        }
    }
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
            custom_hosts: Vec::new(),
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
        let outcome = arm(&[], &[], 25);
        assert!(!outcome.blocked);
        assert!(outcome.reason.is_some());
    }

    #[test]
    fn arming_with_no_duration_is_refused() {
        let outcome = arm(&["social".to_string()], &[], 0);
        assert!(!outcome.blocked);
    }

    fn lock_ending_at(locked_until: i64) -> proto::Lock {
        proto::Lock {
            locked_until,
            armed_at: locked_until - 1_500,
            armed_tick_ms: Some(1_000),
            duration_min: 25,
            categories: vec!["social".into()],
            custom_hosts: Vec::new(),
        }
    }

    fn a_row(ends_ts: i64, state: crate::db::sessions::State) -> crate::db::sessions::SessionRow {
        crate::db::sessions::SessionRow {
            id: 1,
            intention_id: 1,
            task_id: Some(7),
            task_title: Some("write the report".into()),
            started_ts: ends_ts - 1_500,
            ends_ts,
            duration_min: 25,
            enforced: false,
            categories: Some("social".into()),
            predicted_yes: Some(true),
            state,
            ended_ts: None,
            answered_ts: None,
            task_done: false,
        }
    }

    #[test]
    fn nothing_anywhere_is_idle() {
        assert_eq!(reconcile(None, None, 1_000, CHECKIN_GRACE), Reconciled::Idle);
    }

    #[test]
    fn a_running_session_reports_its_own_remainder() {
        let row = a_row(1_600, crate::db::sessions::State::Running);
        assert_eq!(
            reconcile(Some(&row), None, 1_000, CHECKIN_GRACE),
            Reconciled::Running {
                seconds_remaining: 600,
                block_seconds: None,
            }
        );
    }

    #[test]
    fn the_session_and_the_block_are_reported_separately() {
        // They have different owners and different clocks. Fusing them into one
        // number means one of the two is a lie.
        let row = a_row(1_600, crate::db::sessions::State::Running);
        let lock = lock_ending_at(1_900);
        assert_eq!(
            reconcile(Some(&row), Some(&lock), 1_000, CHECKIN_GRACE),
            Reconciled::Running {
                seconds_remaining: 600,
                block_seconds: Some(900),
            }
        );
    }

    #[test]
    fn a_session_that_just_ended_owes_a_check_in() {
        let row = a_row(1_000, crate::db::sessions::State::Running);
        assert_eq!(
            reconcile(Some(&row), None, 1_060, CHECKIN_GRACE),
            Reconciled::CheckinDue {
                late_by: 60,
                block_seconds: None,
            }
        );
    }

    #[test]
    fn the_session_ends_even_while_the_block_still_holds() {
        // The helper may refuse to lift - its monotonic clock still owes time.
        // The session is a commitment to a task, not to the hosts file, so it
        // ends on schedule and the check-in says the sites stay quiet a while.
        let row = a_row(1_000, crate::db::sessions::State::Running);
        let lock = lock_ending_at(2_000);
        assert_eq!(
            reconcile(Some(&row), Some(&lock), 1_060, CHECKIN_GRACE),
            Reconciled::CheckinDue {
                late_by: 60,
                block_seconds: Some(940),
            }
        );
    }

    #[test]
    fn a_session_that_ended_hours_ago_is_not_worth_asking_about() {
        // Slept through it. Asking now is asking someone to score a memory, and
        // the answer would be scored against a real prediction.
        let row = a_row(1_000, crate::db::sessions::State::Running);
        assert_eq!(
            reconcile(Some(&row), None, 1_000 + CHECKIN_GRACE + 1, CHECKIN_GRACE),
            Reconciled::TooLate
        );
    }

    #[test]
    fn a_lock_with_no_session_invents_nothing() {
        // Upgraded mid-commitment, or the database was replaced. There is no
        // task and no prediction to score, so no session is fabricated.
        let lock = lock_ending_at(1_900);
        assert_eq!(
            reconcile(None, Some(&lock), 1_000, CHECKIN_GRACE),
            Reconciled::BlockOnly {
                block_seconds: 900
            }
        );
    }

    #[test]
    fn an_answered_session_is_done_even_if_the_block_outlives_it() {
        let row = a_row(1_000, crate::db::sessions::State::Completed);
        let lock = lock_ending_at(1_900);
        assert_eq!(
            reconcile(Some(&row), Some(&lock), 1_000, CHECKIN_GRACE),
            Reconciled::BlockOnly {
                block_seconds: 900
            }
        );
    }

    #[test]
    fn an_expired_lock_does_not_count_as_a_block() {
        let row = a_row(1_600, crate::db::sessions::State::Running);
        let lock = lock_ending_at(900);
        assert_eq!(
            reconcile(Some(&row), Some(&lock), 1_000, CHECKIN_GRACE),
            Reconciled::Running {
                seconds_remaining: 600,
                block_seconds: None,
            }
        );
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
        let request = proto::Request::block(
            vec!["social".into()],
            vec!["pinterest.com".into()],
            1_800_000_000,
            1_799_999_000,
        );
        let encoded = serde_json::to_string(&request).unwrap();
        let decoded: proto::Request = proto::parse_json(&encoded).unwrap();
        assert_eq!(decoded.action, proto::Action::Block);
        assert_eq!(decoded.until, Some(1_800_000_000));
        assert_eq!(decoded.custom_hosts, vec!["pinterest.com".to_string()]);
        assert!(!decoded.dry_run);
    }
}
