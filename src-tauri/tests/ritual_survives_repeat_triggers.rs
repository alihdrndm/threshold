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

/// The real sleep/wake case: the machine wakes at the lock screen. The wake
/// defers rather than building a ritual nobody can see, so nothing is marked,
/// and the unlock - however long the password takes - is the one that shows.
#[test]
fn a_wake_behind_the_lock_screen_hands_the_ritual_to_the_unlock() {
    let mut state = TriggerState::default();
    state.mark_locked(ago(3_600));
    state.mark_suspended();

    // The wake is evaluated, but the screen is locked, so it is not shown
    // and not marked. Four minutes of typing later:
    assert_eq!(state.evaluate(TriggerKind::Wake, ago(240)), Decision::Show);
    assert_eq!(
        state.evaluate(TriggerKind::Unlock, Instant::now()),
        Decision::Show,
        "the unlock is the first moment anyone could see a ritual"
    );
}

/// The double ritual. A wake whose ritual was actually seen, followed by the
/// unlock task reporting the same return, used to be let through by a
/// three-minute "handover" written for the lock-screen case above, which no
/// longer exists. One return to the machine is one ritual.
#[test]
fn an_unlock_reporting_a_seen_wake_is_not_a_second_ritual() {
    let mut state = TriggerState::default();
    let wake_at = ago(20);
    state.mark_suspended();
    assert_eq!(state.evaluate(TriggerKind::Wake, wake_at), Decision::Show);
    state.mark_shown(wake_at);

    assert_ne!(
        state.evaluate(TriggerKind::Unlock, Instant::now()),
        Decision::Show,
        "the ritual for this return is already on screen"
    );
}

/// The boot case the user reported: the autostart entry and the logon task
/// both launch the app, ten seconds apart, and Settings had the gap below
/// that. Every road into one boot must collapse into one ritual.
#[test]
fn a_boot_is_one_ritual_even_with_the_gap_at_zero() {
    let mut state = TriggerState::with_thresholds(Duration::ZERO, Duration::ZERO);

    let autostart_at = ago(12);
    assert_eq!(
        state.evaluate(TriggerKind::Boot, autostart_at),
        Decision::Show
    );
    state.mark_shown(autostart_at);

    // The logon task, then the resume task a fast-startup boot can set off,
    // then the unlock task on a machine that signs in and locks itself after
    // an update.
    for late in [TriggerKind::Boot, TriggerKind::Wake, TriggerKind::Unlock] {
        assert_eq!(
            state.evaluate(late, Instant::now()),
            Decision::SameReturn,
            "{late:?} after the boot ritual is the same boot"
        );
    }
}

/// And the gap still means what it says: someone who set it to seconds and
/// then locks the machine and comes back is met again.
#[test]
fn a_short_gap_still_meets_the_next_real_return() {
    let mut state = TriggerState::with_thresholds(Duration::ZERO, Duration::from_secs(10));
    state.mark_shown(ago(90));
    state.mark_locked(ago(60));
    assert_eq!(
        state.evaluate(TriggerKind::Unlock, Instant::now()),
        Decision::Show
    );
}

/// And it only applies in that direction: a wake right after an unlock is the
/// duplicate that the debounce exists to swallow.
#[test]
fn a_wake_right_after_an_unlock_is_still_suppressed() {
    let mut state = TriggerState::default();
    let unlock_at = ago(20);
    state.mark_shown(unlock_at);

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
