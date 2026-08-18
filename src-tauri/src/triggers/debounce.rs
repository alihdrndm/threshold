//! When a trigger fires, this decides whether the ritual actually appears.
//!
//! Getting this wrong in either direction breaks the product: too eager and
//! Threshold becomes the nagging dialog it exists to replace; too shy and it
//! misses the boot/wake window the whole design is built around.
//!
//! Everything here is monotonic (`Instant`, not wall-clock) so that changing
//! the system clock cannot manufacture or suppress a prompt.

use std::time::{Duration, Instant};

/// Never show the ritual twice inside this window, whatever fires. The
/// default; Settings can bring it down to seconds for people who want every
/// return met, or up for people who do not.
pub const DEFAULT_MIN_GAP: Duration = Duration::from_secs(15 * 60);

/// An unlock only counts as a "return to the computer" if you were actually
/// away. Stepping out for a coffee is not the same as sitting back down. The
/// default; yours to change.
pub const DEFAULT_LOCK_THRESHOLD: Duration = Duration::from_secs(20 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerKind {
    /// App started at logon.
    Boot,
    /// Resume from sleep or hibernate.
    Wake,
    /// Workstation unlocked.
    Unlock,
    /// User asked for it from the tray.
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Show,
    /// A focus session is running; show a quiet "X min left" note instead.
    SessionInProgress,
    /// Fired again too soon after the last prompt.
    TooSoon,
    /// Unlocked, but the machine had not been locked long enough to count.
    NotAwayLongEnough,
}

/// How long after a wake an unlock still counts as the same return.
///
/// A machine woken at the lock screen cannot display anything: the ritual is
/// created behind the lock and never seen. The unlock moments later is the
/// first opportunity the user actually has, so it is allowed through rather
/// than dismissed as a duplicate. Longer than this and they are two separate
/// returns.
pub const WAKE_HANDOVER: Duration = Duration::from_secs(180);

/// A running session, as the debounce sees it.
#[derive(Debug, Clone, Copy)]
pub struct Session {
    /// Monotonic deadline. Immune to a changed system clock.
    pub until: Instant,
    /// Wall-clock deadline, in unix seconds.
    ///
    /// Carried alongside the monotonic one because `Instant` is
    /// QueryPerformanceCounter, and across suspend it may not advance at all.
    /// A session started before a four-hour sleep would then still look live on
    /// waking, and suppress every trigger until the app restarted - silently
    /// disabling the one moment the product exists for.
    pub ends_ts: i64,
    /// Whether a block was actually confirmed in the hosts file.
    ///
    /// Only an enforced session silences boot, wake and unlock. A block-less
    /// session is a self-report with no evidence behind it, and the likeliest
    /// state at a wake forty minutes in is that the user left and came back -
    /// exactly what the ritual is for.
    pub enforced: bool,
}

#[derive(Debug)]
pub struct TriggerState {
    last_shown: Option<Instant>,
    last_kind: Option<TriggerKind>,
    locked_at: Option<Instant>,
    session: Option<Session>,
    lock_threshold: Duration,
    min_gap: Duration,
}

impl Default for TriggerState {
    fn default() -> Self {
        Self {
            last_shown: None,
            last_kind: None,
            locked_at: None,
            session: None,
            lock_threshold: DEFAULT_LOCK_THRESHOLD,
            min_gap: DEFAULT_MIN_GAP,
        }
    }
}

impl TriggerState {
    pub fn with_thresholds(lock_threshold: Duration, min_gap: Duration) -> Self {
        Self {
            lock_threshold,
            min_gap,
            ..Self::default()
        }
    }

    /// Take new thresholds without losing what has already happened - the
    /// last prompt, the lock in progress, the running session. Settings are
    /// changed while the app runs, and a change must not reset its memory.
    pub fn set_thresholds(&mut self, lock_threshold: Duration, min_gap: Duration) {
        self.lock_threshold = lock_threshold;
        self.min_gap = min_gap;
    }

    pub fn min_gap(&self) -> Duration {
        self.min_gap
    }

    pub fn mark_locked(&mut self, now: Instant) {
        self.locked_at = Some(now);
    }

    /// Undo `mark_shown`, for a ritual that was recorded and then never
    /// appeared. A prompt nobody saw must not count as a prompt.
    pub fn forget_last_shown(&mut self) {
        self.last_shown = None;
    }

    pub fn mark_shown_for(&mut self, kind: TriggerKind, now: Instant) {
        self.last_kind = Some(kind);
        self.mark_shown(now);
    }

    pub fn mark_shown(&mut self, now: Instant) {
        self.last_shown = Some(now);
        // A fresh prompt supersedes the away timer; the next unlock has to earn
        // its own.
        self.locked_at = None;
    }

    pub fn start_session(&mut self, session: Session) {
        self.session = Some(session);
    }

    pub fn end_session(&mut self) {
        self.session = None;
    }

    /// Is a session still running, by both clocks?
    ///
    /// Both must agree. A monotonic clock frozen by suspend can only end a
    /// suppression early this way, never extend it forever; the worst case is
    /// one ritual the user did not strictly need, which is a far better failure
    /// than a product that quietly stops working after a nap.
    fn session_live(&self, now: Instant, now_ts: i64) -> Option<Session> {
        let session = self.session?;
        (now < session.until && now_ts < session.ends_ts).then_some(session)
    }

    pub fn evaluate(&self, kind: TriggerKind, now: Instant) -> Decision {
        self.evaluate_at(kind, now, chrono::Utc::now().timestamp())
    }

    pub fn evaluate_at(&self, kind: TriggerKind, now: Instant, now_ts: i64) -> Decision {
        // A session in progress has its own screen, so nothing else opens one.
        //
        // Only an enforced session stops a trigger, though. Without a block
        // there is no evidence the commitment is still in force, and swallowing
        // boot, wake and unlock for hours on the strength of a click nobody
        // followed through on is how the app disables itself.
        if let Some(session) = self.session_live(now, now_ts) {
            if session.enforced || kind == TriggerKind::Manual {
                return Decision::SessionInProgress;
            }
        }

        if kind == TriggerKind::Manual {
            return Decision::Show;
        }

        if let Some(last) = self.last_shown {
            if now.duration_since(last) < self.min_gap {
                // One exception: an unlock shortly after a wake. The wake's
                // ritual was built behind the lock screen and never seen, so
                // this is the user's first real chance at it, not a duplicate.
                let handing_over = kind == TriggerKind::Unlock
                    && self.last_kind == Some(TriggerKind::Wake)
                    && now.duration_since(last) < WAKE_HANDOVER;
                if !handing_over {
                    return Decision::TooSoon;
                }
            }
        }

        // Modern Standby machines often deliver a wake as an unlock and nothing
        // else, so unlock has to stay a real trigger — but only when the
        // absence was long enough to have broken concentration.
        if kind == TriggerKind::Unlock {
            match self.locked_at {
                Some(locked) if now.duration_since(locked) >= self.lock_threshold => {}
                Some(_) => return Decision::NotAwayLongEnough,
                // No lock was observed (app started after the lock, or the
                // event was missed). Treat it as a genuine return rather than
                // silently swallowing the one trigger that catches S0 wakes.
                None => {}
            }
        }

        Decision::Show
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ago(secs: u64) -> Instant {
        Instant::now()
            .checked_sub(Duration::from_secs(secs))
            .expect("test offset within Instant range")
    }

    #[test]
    fn shows_on_first_boot() {
        let state = TriggerState::default();
        assert_eq!(
            state.evaluate(TriggerKind::Boot, Instant::now()),
            Decision::Show
        );
    }

    #[test]
    fn suppresses_a_second_prompt_inside_the_gap() {
        let mut state = TriggerState::default();
        state.mark_shown(ago(60));
        assert_eq!(
            state.evaluate(TriggerKind::Wake, Instant::now()),
            Decision::TooSoon
        );
    }

    #[test]
    fn allows_a_prompt_once_the_gap_has_passed() {
        let mut state = TriggerState::default();
        state.mark_shown(ago(16 * 60));
        assert_eq!(
            state.evaluate(TriggerKind::Wake, Instant::now()),
            Decision::Show
        );
    }

    #[test]
    fn the_thresholds_are_yours_and_can_be_seconds() {
        // Ten seconds of gap, five seconds of lock: every return is met.
        let mut state =
            TriggerState::with_thresholds(Duration::from_secs(5), Duration::from_secs(10));
        state.mark_shown(ago(11));
        assert_eq!(state.evaluate(TriggerKind::Wake, Instant::now()), Decision::Show);
        state.mark_shown(ago(3));
        assert_eq!(state.evaluate(TriggerKind::Wake, Instant::now()), Decision::TooSoon);

        state.mark_shown(ago(60));
        state.mark_locked(ago(6));
        assert_eq!(
            state.evaluate(TriggerKind::Unlock, Instant::now()),
            Decision::Show,
            "six seconds locked clears a five-second threshold"
        );
    }

    #[test]
    fn changing_thresholds_keeps_what_already_happened() {
        let mut state = TriggerState::default();
        state.mark_shown(ago(30));
        state.set_thresholds(Duration::from_secs(5), Duration::from_secs(20));
        assert_eq!(
            state.evaluate(TriggerKind::Wake, Instant::now()),
            Decision::Show,
            "the last prompt is remembered and the new, shorter gap is honoured"
        );
        state.set_thresholds(Duration::from_secs(5), Duration::from_secs(60));
        assert_eq!(state.evaluate(TriggerKind::Wake, Instant::now()), Decision::TooSoon);
    }

    #[test]
    fn a_brief_lock_does_not_count_as_returning() {
        let mut state = TriggerState::default();
        state.mark_locked(ago(5 * 60));
        assert_eq!(
            state.evaluate(TriggerKind::Unlock, Instant::now()),
            Decision::NotAwayLongEnough
        );
    }

    #[test]
    fn a_long_lock_does_count_as_returning() {
        let mut state = TriggerState::default();
        state.mark_locked(ago(25 * 60));
        assert_eq!(
            state.evaluate(TriggerKind::Unlock, Instant::now()),
            Decision::Show
        );
    }

    #[test]
    fn unlock_without_an_observed_lock_still_prompts() {
        // The S0 case: the app may never have seen the lock event.
        let state = TriggerState::default();
        assert_eq!(
            state.evaluate(TriggerKind::Unlock, Instant::now()),
            Decision::Show
        );
    }

    fn session(secs: u64, enforced: bool) -> Session {
        Session {
            until: Instant::now() + Duration::from_secs(secs),
            ends_ts: chrono::Utc::now().timestamp() + secs as i64,
            enforced,
        }
    }

    #[test]
    fn a_blocked_session_suppresses_everything_including_manual() {
        let mut state = TriggerState::default();
        state.start_session(session(600, true));
        for kind in [
            TriggerKind::Boot,
            TriggerKind::Wake,
            TriggerKind::Unlock,
            TriggerKind::Manual,
        ] {
            assert_eq!(
                state.evaluate(kind, Instant::now()),
                Decision::SessionInProgress
            );
        }
    }

    #[test]
    fn a_blockless_session_still_lets_the_ritual_interrupt() {
        // Without a block there is no evidence the commitment is still in
        // force. Suppressing wake and unlock on the strength of a click nobody
        // followed through on is the app quietly switching itself off.
        let mut state = TriggerState::default();
        state.start_session(session(600, false));

        for kind in [TriggerKind::Boot, TriggerKind::Wake] {
            assert_eq!(state.evaluate(kind, Instant::now()), Decision::Show);
        }
        // But a second Focus click is still refused: that one has a screen.
        assert_eq!(
            state.evaluate(TriggerKind::Manual, Instant::now()),
            Decision::SessionInProgress
        );
    }

    #[test]
    fn an_expired_session_stops_suppressing() {
        let mut state = TriggerState::default();
        state.start_session(Session {
            until: ago(1),
            ends_ts: chrono::Utc::now().timestamp() - 1,
            enforced: true,
        });
        assert_eq!(
            state.evaluate(TriggerKind::Manual, Instant::now()),
            Decision::Show
        );
    }

    #[test]
    fn a_frozen_monotonic_clock_cannot_suppress_forever() {
        // The suspend case. `Instant` is QueryPerformanceCounter and may not
        // advance across sleep, so a session started before a long nap can
        // still look live on the monotonic clock. The wall clock breaks the
        // tie, and the trigger gets through.
        let mut state = TriggerState::default();
        state.start_session(Session {
            until: Instant::now() + Duration::from_secs(3_600),
            ends_ts: chrono::Utc::now().timestamp() - 1,
            enforced: true,
        });
        assert_eq!(
            state.evaluate(TriggerKind::Wake, Instant::now()),
            Decision::Show
        );
    }

    #[test]
    fn a_changed_wall_clock_cannot_suppress_forever_either() {
        // And the mirror image: winding the clock forward does not manufacture
        // a suppression, because the monotonic side still has to agree.
        let mut state = TriggerState::default();
        state.start_session(Session {
            until: ago(1),
            ends_ts: chrono::Utc::now().timestamp() + 3_600,
            enforced: true,
        });
        assert_eq!(
            state.evaluate(TriggerKind::Wake, Instant::now()),
            Decision::Show
        );
    }

    #[test]
    fn asking_for_it_ignores_the_gap() {
        let mut state = TriggerState::default();
        state.mark_shown(ago(30));
        assert_eq!(
            state.evaluate(TriggerKind::Manual, Instant::now()),
            Decision::Show
        );
    }

    #[test]
    fn showing_the_prompt_resets_the_away_timer() {
        let mut state = TriggerState::default();
        state.mark_locked(ago(30 * 60));
        state.mark_shown(ago(20 * 60));
        // Long enough since the prompt, but the old lock must not count again.
        assert_eq!(
            state.evaluate(TriggerKind::Unlock, Instant::now()),
            Decision::Show
        );
    }
}
