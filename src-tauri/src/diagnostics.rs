//! Whether this installation can actually do its job.
//!
//! Every failure the user hit was silent: the block was never armed, the tasks
//! pointed at a build that had moved, the helper was never registered. None of
//! it surfaced anywhere. This is the one place that answers "is it set up?"
//! honestly, for both `--doctor` and the Settings panel.

use serde::Serialize;

use crate::{session, triggers::scheduled_tasks};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    pub checks: Vec<Check>,
    /// True when nothing needs the user's attention.
    pub healthy: bool,
    /// True when blocking specifically cannot work, which needs elevation to fix.
    pub needs_repair: bool,
    /// True when a pause is swallowing every trigger.
    pub paused: bool,
}

impl Diagnostics {
    pub fn report(&self) -> String {
        let mut out = String::new();
        for check in &self.checks {
            out.push_str(&format!(
                "{} {}\n    {}\n",
                if check.ok { "OK  " } else { "FAIL" },
                check.name,
                check.detail
            ));
        }
        // Ordered by what actually stops the app working, most likely first.
        out.push_str(if self.paused {
            "\nThreshold is paused, so no ritual will appear at boot, wake or unlock \
             however healthy everything else looks.\nResume it from the tray or from \
             Settings.\n"
        } else if self.needs_repair {
            "\nBlocking cannot work until the helper is registered. \
             Run as administrator:\n    threshold.exe --register-helper\n"
        } else if self.healthy {
            "\nEverything needed is in place.\n"
        } else {
            "\nSome triggers are not set up; they will be registered next time the app starts.\n"
        });
        out
    }
}

/// Diagnostics for the command line, where no app state exists yet.
///
/// Reads the pause directly from the database rather than reporting `None`.
/// A `--doctor` that says "everything needed is in place" while a pause is
/// silently swallowing every trigger is worse than no diagnostic at all — it
/// actively points the investigation away from the cause.
pub fn diagnostics() -> Diagnostics {
    let paused = crate::db::open()
        .ok()
        .and_then(|conn| crate::pause::paused_until(&conn));
    diagnostics_with_pause(paused)
}

fn format_unix(seconds: i64) -> String {
    chrono::DateTime::from_timestamp(seconds, 0)
        .map(|dt| {
            chrono::DateTime::<chrono::Local>::from(dt)
                .format("%-d %B, %H:%M")
                .to_string()
        })
        .unwrap_or_else(|| seconds.to_string())
}

pub fn diagnostics_with_pause(paused_until: Option<i64>) -> Diagnostics {
    let mut checks = Vec::new();

    let exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".into());
    checks.push(Check {
        name: "Running from".into(),
        ok: true,
        detail: exe.clone(),
    });

    // The helper binary must exist beside the app; a task pointing at a missing
    // file registers happily and then does nothing.
    let helper = scheduled_tasks::helper_path();
    let helper_present = helper.is_file();
    checks.push(Check {
        name: "Helper binary".into(),
        ok: helper_present,
        detail: if helper_present {
            helper.to_string_lossy().to_string()
        } else {
            format!("missing at {}", helper.display())
        },
    });

    let helper_ok = scheduled_tasks::helper_usable();
    checks.push(Check {
        name: "Blocking helper task".into(),
        ok: helper_ok,
        detail: match scheduled_tasks::helper_task_target() {
            Some(target) if helper_ok => format!("registered, runs {target}"),
            Some(target) => format!("registered but its target is missing: {target}"),
            None => "not registered - blocking will do nothing".into(),
        },
    });

    // Relaunch triggers: present, and aimed at this build.
    let exe_stem = exe.to_lowercase();
    let exe_stem = exe_stem.trim_end_matches(".exe");
    for (name, target) in scheduled_tasks::registered_targets() {
        let (ok, detail) = match target {
            None => (false, "not registered".to_string()),
            Some(path) if path.to_lowercase().contains(exe_stem) => {
                (true, format!("runs {path}"))
            }
            Some(path) => (
                false,
                format!("points at a different build: {path}"),
            ),
        };
        checks.push(Check {
            name: format!("Trigger: {name}"),
            ok,
            detail,
        });
    }

    // A pause suppresses every trigger. That is correct behaviour and a silent
    // week of it is indistinguishable from the app being broken, so it is
    // reported first among the things that stop it working.
    if let Some(until) = paused_until {
        checks.push(Check {
            name: "Paused".into(),
            ok: false,
            detail: format!(
                "no ritual will appear until {}. Resume it in Settings.",
                format_unix(until)
            ),
        });
    }

    let blocked_now = threshold_protocol::hosts_has_block();
    let lock = session::active_lock();
    checks.push(Check {
        name: "Current session".into(),
        ok: true,
        detail: match (&lock, blocked_now) {
            (Some(lock), _) => format!(
                "{} minutes committed, {} remaining, blocking [{}]",
                lock.duration_min,
                session::seconds_remaining(lock) / 60,
                lock.categories.join(", ")
            ),
            (None, true) => {
                "hosts file is blocked but no lock exists - run --unblock-now to clear".into()
            }
            (None, false) => "none".into(),
        },
    });

    let needs_repair = !helper_ok || !helper_present;
    let healthy = checks.iter().all(|check| check.ok);
    let paused = paused_until.is_some();

    Diagnostics {
        checks,
        healthy,
        needs_repair,
        paused,
    }
}
