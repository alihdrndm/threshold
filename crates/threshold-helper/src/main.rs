//! Threshold's elevated helper.
//!
//! Security contract, fixed because it is a boundary: this binary runs from a
//! "run with highest privileges" scheduled task, so anything able to trigger
//! that task would control its arguments. It therefore accepts none.
//! Instructions come only from a schema-checked file at a fixed path.
//!
//! Run it with a request whose `dry_run` is true and it computes everything,
//! changes nothing, and prints exactly what it would have done. That mode needs
//! no elevation at all, which is what makes it safe to review.

mod blocklist;
mod hosts;
mod lock;
mod policies;
mod request;

use std::path::PathBuf;

use request::{Action, Request};

fn main() {
    // Deliberately ignoring std::env::args(). See the module note above.
    let path = request::request_path();

    let req = match request::load(&path) {
        Ok(req) => req,
        Err(err) => {
            eprintln!("threshold-helper: {err}");
            std::process::exit(2);
        }
    };

    match run(&req) {
        Ok(report) => {
            println!("{report}");
            std::process::exit(0);
        }
        Err(err) => {
            eprintln!("threshold-helper: {err}");
            std::process::exit(1);
        }
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}

fn lock_path() -> PathBuf {
    request::program_data().join(lock::LOCK_FILE)
}

fn read_lock() -> lock::Stored {
    lock::load(&lock_path())
}

fn run(req: &Request) -> Result<String, String> {
    match req.action {
        Action::Block => block(req),
        Action::Unblock => unblock(req, false),
        Action::EmergencyUnblock => unblock(req, true),
    }
}

fn block(req: &Request) -> Result<String, String> {
    let targets = blocklist::hosts_for(&req.categories);
    let unknown = blocklist::unknown_categories(&req.categories);
    let hosts_path = hosts::system_hosts();
    let existing = hosts::read(&hosts_path)?;
    let proposed = hosts::with_block(&existing, &targets);

    let until = req.until.unwrap_or_default();
    let armed_at = now_secs();
    let new_lock = lock::Lock {
        locked_until: until,
        armed_at,
        armed_tick_ms: Some(lock::tick_ms()),
        duration_min: ((until - armed_at) / 60).max(0),
        categories: req.categories.clone(),
    };

    if req.dry_run {
        return Ok(dry_run_report(
            "block",
            &existing,
            &proposed,
            &targets,
            &unknown,
            Some(&new_lock),
            true,
        ));
    }

    hosts::write_atomic(&hosts_path, &proposed)?;
    policies::apply()?;
    flush_dns();

    std::fs::create_dir_all(request::program_data())
        .map_err(|err| format!("could not create state directory: {err}"))?;
    std::fs::write(lock_path(), lock::render(&new_lock))
        .map_err(|err| format!("could not write lock: {err}"))?;

    Ok(format!(
        "blocked {} hosts across {} categories until {}",
        targets.len(),
        req.categories.len(),
        until
    ))
}

fn unblock(req: &Request, emergency: bool) -> Result<String, String> {
    let current = read_lock();
    let verdict = lock::evaluate(&current, now_secs(), lock::tick_ms());

    // The commitment lock. A normal unblock before time is up is refused - this
    // is the one thing the UI is not allowed to talk us out of. A lock that
    // cannot be read is refused too: failing open would make corrupting the
    // file the easiest way out of a commitment.
    if !emergency {
        match verdict {
            lock::Verdict::Held { seconds_remaining } => {
                return Err(format!(
                    "refused: {} minutes still committed",
                    (seconds_remaining + 59) / 60
                ))
            }
            lock::Verdict::Unreadable => {
                return Err(
                    "refused: the lock file exists but cannot be read, so the commitment \
                     is treated as still running. Use the emergency path if you need out."
                        .to_string(),
                )
            }
            _ => {}
        }
    }

    let hosts_path = hosts::system_hosts();
    let existing = hosts::read(&hosts_path)?;
    let proposed = hosts::with_block(&existing, &[]);

    if req.dry_run {
        return Ok(dry_run_report(
            if emergency { "emergency_unblock" } else { "unblock" },
            &existing,
            &proposed,
            &[],
            &[],
            None,
            false,
        ));
    }

    hosts::write_atomic(&hosts_path, &proposed)?;
    policies::remove()?;
    flush_dns();
    let _ = std::fs::remove_file(lock_path());

    // Every emergency exit is recorded — neutrally, as a count, never as a
    // reprimand. Shame is what makes people abandon the tool entirely.
    if emergency {
        let seconds_early = match verdict {
            lock::Verdict::Held { seconds_remaining } => seconds_remaining,
            _ => 0,
        };
        let event = lock::DriftEvent {
            ts: now_secs(),
            seconds_early,
        };
        let _ = std::fs::write(
            request::program_data().join(lock::DRIFT_FILE),
            serde_json::to_string(&event).unwrap_or_default(),
        );
    }

    Ok(if emergency {
        "unblocked early; drift recorded".to_string()
    } else {
        "unblocked".to_string()
    })
}

/// Browsers cache DNS internally, so an open tab may survive until refresh.
/// Documented rather than fought.
fn flush_dns() {
    let _ = std::process::Command::new("ipconfig").arg("/flushdns").output();
}

#[allow(clippy::too_many_arguments)]
fn dry_run_report(
    action: &str,
    existing: &str,
    proposed: &str,
    targets: &[String],
    unknown: &[String],
    new_lock: Option<&lock::Lock>,
    applying_policies: bool,
) -> String {
    let mut out = String::new();
    out.push_str("DRY RUN — nothing was changed\n");
    out.push_str(&format!("action: {action}\n\n"));

    out.push_str(&format!(
        "hosts file: {}\n  {} lines now, {} lines proposed\n",
        hosts::system_hosts().display(),
        existing.lines().count(),
        proposed.lines().count(),
    ));

    if !targets.is_empty() {
        out.push_str(&format!("  {} hosts would be blocked, e.g.\n", targets.len()));
        for host in targets.iter().take(6) {
            out.push_str(&format!("    0.0.0.0 {host}\n"));
        }
        if targets.len() > 6 {
            out.push_str(&format!("    … and {} more\n", targets.len() - 6));
        }
    } else {
        out.push_str("  the Threshold block would be removed entirely\n");
    }

    if !unknown.is_empty() {
        out.push_str(&format!(
            "  warning: unrecognised categories ignored: {}\n",
            unknown.join(", ")
        ));
    }

    out.push_str("\nregistry:\n");
    for line in policies::describe() {
        out.push_str(&format!(
            "  {} {line}\n",
            if applying_policies { "SET   " } else { "DELETE" }
        ));
    }

    out.push_str("\nlines outside the Threshold markers are preserved exactly:\n");
    let preserved = hosts::without_block(existing);
    for line in preserved.lines().take(4) {
        out.push_str(&format!("    {line}\n"));
    }

    if let Some(lock) = new_lock {
        out.push_str(&format!(
            "\nlock: {} would hold for {} minutes (until {})\n",
            lock_path().display(),
            lock.duration_min,
            lock.locked_until
        ));
    } else {
        out.push_str(&format!("\nlock: {} would be removed\n", lock_path().display()));
    }

    out.push_str("\nipconfig /flushdns would run afterwards\n");
    out
}
