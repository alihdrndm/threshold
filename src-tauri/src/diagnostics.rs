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
        out.push_str(if self.healthy {
            "\nEverything needed is in place.\n"
        } else if self.needs_repair {
            "\nBlocking cannot work until the helper is registered. \
             Run as administrator:\n    threshold.exe --register-helper\n"
        } else {
            "\nSome triggers are not set up; they will be registered next time the app starts.\n"
        });
        out
    }
}

pub fn diagnostics() -> Diagnostics {
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

    Diagnostics {
        checks,
        healthy,
        needs_repair,
    }
}
