//! When a trigger fires, this decides whether the ritual actually appears.
//!
//! Getting this wrong in either direction breaks the product: too eager and
//! Threshold becomes the nagging dialog it exists to replace; too shy and it
//! misses the boot/wake window the whole design is built around.
//!
//! Everything here is monotonic (`Instant`, not wall-clock) so that changing
//! the system clock cannot manufacture or suppress a prompt.

use std::time::{Duration, Instant};

/// Never show the ritual twice inside this window, whatever fires.
pub const MIN_GAP: Duration = Duration::from_secs(15 * 60);

/// An unlock only counts as a "return to the computer" if you were actually
/// away. Stepping out for a coffee is not the same as sitting back down.
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

#[derive(Debug)]
pub struct TriggerState {
    last_shown: Option<Instant>,
    locked_at: Option<Instant>,
    session_until: Option<Instant>,
    lock_threshold: Duration,
}

impl Default for TriggerState {
    fn default() -> Self {
        Self {
            last_shown: None,
            locked_at: None,
            session_until: None,
            lock_threshold: DEFAULT_LOCK_THRESHOLD,
        }
    }
}

impl TriggerState {
    pub fn with_lock_threshold(lock_threshold: Duration) -> Self {
        Self {
            lock_threshold,
            ..Self::default()
        }
    }

    pub fn mark_locked(&mut self, now: Instant) {
        self.locked_at = Some(now);
    }

    pub fn mark_shown(&mut self, now: Instant) {
        self.last_shown = Some(now);
        // A fresh prompt supersedes the away timer; the next unlock has to earn
        // its own.
        self.locked_at = None;
    }

    pub fn start_session(&mut self, until: Instant) {
        self.session_until = Some(until);
    }

    pub fn end_session(&mut self) {
        self.session_until = None;
    }

    pub fn evaluate(&self, kind: TriggerKind, now: Instant) -> Decision {
        // Asking for it explicitly overrides every guard except an active
        // session, which has its own screen.
        if let Some(until) = self.session_until {
            if now < until {
                return Decision::SessionInProgress;
            }
        }

        if kind == TriggerKind::Manual {
            return Decision::Show;
        }

        if let Some(last) = self.last_shown {
            if now.duration_since(last) < MIN_GAP {
                return Decision::TooSoon;
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

    #[test]
    fn an_active_session_suppresses_everything_including_manual() {
        let mut state = TriggerState::default();
        state.start_session(Instant::now() + Duration::from_secs(600));
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
    fn an_expired_session_stops_suppressing() {
        let mut state = TriggerState::default();
        state.start_session(ago(1));
        assert_eq!(
            state.evaluate(TriggerKind::Manual, Instant::now()),
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
