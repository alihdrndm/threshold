//! The contract between the app and the elevated helper.
//!
//! These types live in one place because they cross a process boundary that is
//! also a privilege boundary. When the app and the helper each kept their own
//! copy, nothing forced them to agree — and nothing noticed that the app never
//! wrote a request at all.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const PROGRAM_DATA: &str = r"C:\ProgramData\Threshold";
pub const REQUEST_FILE: &str = "request.json";
pub const LOCK_FILE: &str = "lock.json";
pub const DRIFT_FILE: &str = "drift.json";

/// Marks Threshold's own section of the hosts file. The app reads this to
/// confirm a block actually landed rather than assuming it did.
pub const HOSTS_BEGIN: &str = "# THRESHOLD-BEGIN";
pub const HOSTS_END: &str = "# THRESHOLD-END";

pub fn program_data() -> PathBuf {
    PathBuf::from(PROGRAM_DATA)
}

pub fn request_path() -> PathBuf {
    program_data().join(REQUEST_FILE)
}

/// Admin-only subdirectory holding the lock. The request file stays writable by
/// the ordinary user — that is how the UI asks for anything — but the lock must
/// not be, or you could shorten your own commitment by editing it.
pub fn state_dir() -> PathBuf {
    program_data().join("state")
}

pub fn lock_path() -> PathBuf {
    state_dir().join(LOCK_FILE)
}

pub fn system_hosts() -> PathBuf {
    PathBuf::from(r"C:\Windows\System32\drivers\etc\hosts")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Block,
    Unblock,
    /// Deliberately available, deliberately costly. Users who defeat a tool via
    /// workarounds abandon self-regulation wholesale, so an official exit is
    /// safer than one they have to invent.
    EmergencyUnblock,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Request {
    pub action: Action,
    #[serde(default)]
    pub categories: Vec<String>,
    /// Unix seconds. Required for `block`, meaningless otherwise.
    #[serde(default)]
    pub until: Option<i64>,
    /// Compute and report the changes without applying any of them.
    #[serde(default)]
    pub dry_run: bool,
}

impl Request {
    pub fn block(categories: Vec<String>, until: i64) -> Self {
        Self {
            action: Action::Block,
            categories,
            until: Some(until),
            dry_run: false,
        }
    }

    pub fn unblock() -> Self {
        Self {
            action: Action::Unblock,
            categories: Vec::new(),
            until: None,
            dry_run: false,
        }
    }

    pub fn emergency_unblock() -> Self {
        Self {
            action: Action::EmergencyUnblock,
            categories: Vec::new(),
            until: None,
            dry_run: false,
        }
    }
}

/// Reject anything that does not describe a coherent action.
///
/// Lives here rather than in the helper so the side that *writes* requests is
/// checked against the same rules as the side that reads them. The helper still
/// validates on receipt — it holds administrator rights and trusts nobody — but
/// the app can no longer send something that will be silently thrown away.
pub fn validate(request: &Request) -> Result<(), String> {
    if request.action == Action::Block {
        match request.until {
            None => return Err("a block must say when it ends".into()),
            Some(until) if until <= 0 => {
                return Err("block end is not a valid time".into())
            }
            _ => {}
        }
        if request.categories.is_empty() {
            return Err("a block must name at least one category".into());
        }
    }

    // Category names index a fixed table; anything else is a typo or an attempt
    // to smuggle something through.
    for category in &request.categories {
        if !category.chars().all(|c| c.is_ascii_lowercase() || c == '-') {
            return Err(format!("unrecognised category name: {category}"));
        }
    }

    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lock {
    /// Unix seconds when the block may be lifted.
    pub locked_until: i64,
    /// Unix seconds when it was armed.
    pub armed_at: i64,
    /// Milliseconds since boot when it was armed. Immune to clock changes.
    /// Optional so a lock written before this existed — or by a tool that emits
    /// `null` — still parses rather than being treated as corrupt.
    #[serde(default)]
    pub armed_tick_ms: Option<u64>,
    /// Minutes committed to.
    pub duration_min: i64,
    pub categories: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftEvent {
    pub ts: i64,
    pub seconds_early: i64,
}

/// Strip a UTF-8 BOM before parsing. Several Windows tools prepend one and JSON
/// parsers reject it; whose editor wrote the file is not something either side
/// should care about.
pub fn parse_json<T: for<'de> Deserialize<'de>>(raw: &str) -> Result<T, String> {
    serde_json::from_str(raw.trim_start_matches('\u{feff}'))
        .map_err(|err| format!("could not parse: {err}"))
}

/// Does the hosts file currently carry Threshold's block?
///
/// The app uses this to verify that asking the helper to block actually
/// achieved something, instead of reporting success it has not confirmed.
pub fn hosts_has_block() -> bool {
    std::fs::read_to_string(system_hosts())
        .map(|contents| contents.contains(HOSTS_BEGIN))
        .unwrap_or(false)
}
