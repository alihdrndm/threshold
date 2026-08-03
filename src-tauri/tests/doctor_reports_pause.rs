//! A pause must never be invisible to the diagnostic.
//!
//! `--doctor` once reported "everything needed is in place" while a week-long
//! pause was swallowing every trigger. That is worse than having no diagnostic:
//! it points the investigation away from the cause. These pin the behaviour so
//! it cannot regress quietly.

use threshold_lib::diagnostics::diagnostics_with_pause;

#[test]
fn a_pause_is_reported_as_a_failing_check() {
    let future = chrono::Utc::now().timestamp() + 7 * 86_400;
    let report = diagnostics_with_pause(Some(future));

    assert!(report.paused, "the pause must be flagged on the struct");
    assert!(
        !report.healthy,
        "an app that will not respond to any trigger is not healthy"
    );

    let paused_check = report
        .checks
        .iter()
        .find(|check| check.name == "Paused")
        .expect("a pause deserves its own check, not a footnote");
    assert!(!paused_check.ok);
    assert!(
        paused_check.detail.to_lowercase().contains("resume"),
        "the check must say how to undo it, not merely that it is true"
    );
}

#[test]
fn the_summary_leads_with_the_pause_rather_than_burying_it() {
    let future = chrono::Utc::now().timestamp() + 86_400;
    let text = diagnostics_with_pause(Some(future)).report();
    assert!(
        text.contains("paused"),
        "the closing summary must name the pause: {text}"
    );
    assert!(
        !text.contains("Everything needed is in place"),
        "a paused app must never be described as fully in place"
    );
}

#[test]
fn no_pause_leaves_no_pause_check() {
    let report = diagnostics_with_pause(None);
    assert!(!report.paused);
    assert!(!report.checks.iter().any(|check| check.name == "Paused"));
}
