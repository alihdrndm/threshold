//! The ritual has to appear on the *second* trigger too.
//!
//! Every earlier test fired one trigger at a freshly started process, so the
//! popup window was always built from nothing and always worked. Real use is
//! the opposite: one long-lived process receives wake after unlock after wake
//! for days. A stale hidden window from the first ritual made every later one
//! silently do nothing, and no test noticed because no test ever triggered
//! twice.
//!
//! These cover the debounce half of that failure — the half that turned "the
//! window did not show" into "and nothing will ever show again". The window
//! half needs a live webview and is covered by the manual matrix in TESTING.md.

use std::time::{Duration, Instant};

use threshold_lib::triggers::debounce::{Decision, TriggerKind, TriggerState};

fn ago(secs: u64) -> Instant {
    Instant::now()
        .checked_sub(Duration::from_secs(secs))
        .expect("test offset within Instant range")
}

/// The core of the bug: if a ritual is recorded as shown when it never reached
/// the screen, the unlock that follows a wake is dismissed as too soon and the
/// user sees nothing at all.
#[test]
fn a_ritual_that_never_appeared_must_not_block_the_next_trigger() {
    let state = TriggerState::default();

    // A wake arrives while the session is still locked. The window cannot be
    // displayed, so nothing is marked as shown.
    let wake_at = ago(60);
    assert_eq!(state.evaluate(TriggerKind::Wake, wake_at), Decision::Show);
    // deliberately no mark_shown - that is the fix

    // The user unlocks a minute later. This must still prompt.
    assert_eq!(
        state.evaluate(TriggerKind::Unlock, Instant::now()),
        Decision::Show,
        "an undisplayed ritual must not consume the debounce window"
    );
}

/// The real sleep/wake case: the machine wakes at the lock screen, so the
/// ritual is built where nobody can see it. The unlock moments later is the
/// user's first actual opportunity and must be allowed through.
#[test]
fn an_unlock_just_after_a_wake_still_shows_the_ritual() {
    let mut state = TriggerState::default();

    let wake_at = ago(20);
    assert_eq!(state.evaluate(TriggerKind::Wake, wake_at), Decision::Show);
    state.mark_shown_for(TriggerKind::Wake, wake_at);

    assert_eq!(
        state.evaluate(TriggerKind::Unlock, Instant::now()),
        Decision::Show,
        "the wake happened behind the lock screen; the unlock is the first real chance"
    );
}

/// That handover is narrow. Well after the wake, an unlock is a duplicate again.
#[test]
fn the_handover_does_not_last_forever() {
    let mut state = TriggerState::default();
    let wake_at = ago(240);
    state.mark_shown_for(TriggerKind::Wake, wake_at);

    assert_eq!(
        state.evaluate(TriggerKind::Unlock, Instant::now()),
        Decision::TooSoon,
        "four minutes later is a separate return, still inside the fifteen minute gap"
    );
}

/// And it only applies in that direction: a wake right after an unlock is the
/// duplicate that the debounce exists to swallow.
#[test]
fn a_wake_right_after_an_unlock_is_still_suppressed() {
    let mut state = TriggerState::default();
    let unlock_at = ago(20);
    state.mark_shown_for(TriggerKind::Unlock, unlock_at);

    assert_eq!(
        state.evaluate(TriggerKind::Wake, Instant::now()),
        Decision::TooSoon,
        "one return to the machine is one ritual, not two"
    );
}

/// Several failed attempts in a row must not accumulate into a suppression.
#[test]
fn repeated_failures_keep_letting_the_next_trigger_through() {
    let state = TriggerState::default();
    for offset in [300, 240, 180, 120, 60] {
        assert_eq!(
            state.evaluate(TriggerKind::Wake, ago(offset)),
            Decision::Show,
            "every attempt should be allowed while none of them display"
        );
    }
    assert_eq!(
        state.evaluate(TriggerKind::Unlock, Instant::now()),
        Decision::Show
    );
}

/// Once one finally displays, normal debouncing resumes immediately.
#[test]
fn the_first_successful_display_restores_normal_debouncing() {
    let mut state = TriggerState::default();
    assert_eq!(state.evaluate(TriggerKind::Wake, ago(120)), Decision::Show);

    let shown_at = ago(30);
    assert_eq!(state.evaluate(TriggerKind::Unlock, shown_at), Decision::Show);
    state.mark_shown(shown_at);

    assert_eq!(
        state.evaluate(TriggerKind::Wake, Instant::now()),
        Decision::TooSoon
    );
}
