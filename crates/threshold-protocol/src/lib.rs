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
/// Whatever the browser blocklist keys held before Threshold touched them.
///
/// Its presence is the marker that browser policy is currently applied. The app
/// cannot read the registry keys the helper writes — that is the helper's side
/// of the privilege boundary — but it can see this, and a file here with no lock
/// beside it means policy is in force with no commitment behind it.
pub const POLICY_BACKUP_FILE: &str = "policy-backup.json";

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

pub fn policy_backup_path() -> PathBuf {
    state_dir().join(POLICY_BACKUP_FILE)
}

/// Is browser policy currently in force because of us?
pub fn policy_applied() -> bool {
    policy_backup_path().is_file()
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

/// How many sites of their own a user may add.
///
/// Not a technical limit — Chromium's `URLBlocklist` takes 1000 entries and the
/// hosts file takes as many as you like. It is a limit on what a blocklist can
/// usefully be: past this you are not naming your distractions, you are building
/// a firewall, and a firewall is a different product.
pub const MAX_CUSTOM_HOSTS: usize = 64;

/// How old a block request may be before the helper refuses it.
///
/// The request file is a single fixed path that survives being acted on, so a
/// stray run of the helper task would otherwise re-apply whatever it last held —
/// with a fresh `armed_at`, turning a finished commitment into a brand new one.
/// The helper deletes the request after reading it; this is the second lock on
/// that door.
pub const MAX_REQUEST_AGE: i64 = 300;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Request {
    pub action: Action,
    #[serde(default)]
    pub categories: Vec<String>,
    /// Sites the user named themselves, as bare apexes (`pinterest.com`).
    ///
    /// Kept separate from `categories` rather than smuggled in as pseudo-category
    /// names: categories index a fixed table the helper owns, these are free text
    /// from a text field, and the two deserve different validation.
    #[serde(default)]
    pub custom_hosts: Vec<String>,
    /// Unix seconds. Required for `block`, meaningless otherwise.
    #[serde(default)]
    pub until: Option<i64>,
    /// Unix seconds when the app wrote this. Absent on requests written by hand,
    /// which is why staleness is only enforced where replaying would do harm.
    #[serde(default)]
    pub issued_at: Option<i64>,
    /// Compute and report the changes without applying any of them.
    #[serde(default)]
    pub dry_run: bool,
}

impl Request {
    pub fn block(categories: Vec<String>, custom_hosts: Vec<String>, until: i64, now: i64) -> Self {
        Self {
            action: Action::Block,
            categories,
            custom_hosts,
            until: Some(until),
            issued_at: Some(now),
            dry_run: false,
        }
    }

    pub fn unblock() -> Self {
        Self {
            action: Action::Unblock,
            categories: Vec::new(),
            custom_hosts: Vec::new(),
            until: None,
            issued_at: None,
            dry_run: false,
        }
    }

    pub fn emergency_unblock() -> Self {
        Self {
            action: Action::EmergencyUnblock,
            categories: Vec::new(),
            custom_hosts: Vec::new(),
            until: None,
            issued_at: None,
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
        if request.categories.is_empty() && request.custom_hosts.is_empty() {
            return Err("a block must name at least one category or site".into());
        }
    }

    // Category names index a fixed table; anything else is a typo or an attempt
    // to smuggle something through.
    for category in &request.categories {
        if !category.chars().all(|c| c.is_ascii_lowercase() || c == '-') {
            return Err(format!("unrecognised category name: {category}"));
        }
    }

    if request.custom_hosts.len() > MAX_CUSTOM_HOSTS {
        return Err(format!(
            "too many sites: {} (the limit is {MAX_CUSTOM_HOSTS})",
            request.custom_hosts.len()
        ));
    }
    for host in &request.custom_hosts {
        valid_hostname(host)?;
    }

    Ok(())
}

/// Is this a hostname we are willing to hand to an elevated process?
///
/// The helper writes whatever passes here straight into the hosts file and the
/// registry, so this is a boundary check, not a nicety. It deliberately demands
/// an already-normalised name — lowercase, no scheme, no port, no path — rather
/// than cleaning input up on the way through: a validator that silently rewrites
/// its input cannot tell you what it actually blocked.
///
/// Requiring the last label to be alphabetic rejects `1.2.3.4` without needing to
/// parse addresses, and `localhost` fails the two-label rule.
pub fn valid_hostname(host: &str) -> Result<(), String> {
    if host.is_empty() {
        return Err("that is empty".into());
    }
    // 253 is the DNS limit for a presentation-format name.
    if host.len() > 253 {
        return Err(format!("that name is too long: {} characters", host.len()));
    }
    if host != host.to_ascii_lowercase() {
        return Err(format!("{host} is not lowercase"));
    }

    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() < 2 {
        return Err(format!(
            "{host} needs at least a name and a suffix, like example.com"
        ));
    }

    for label in &labels {
        if label.is_empty() {
            return Err(format!("{host} has an empty part"));
        }
        if label.len() > 63 {
            return Err(format!("{host} has a part longer than 63 characters"));
        }
        if !label
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(format!("{host} has characters that cannot be in a web address"));
        }
        if label.starts_with('-') || label.ends_with('-') {
            return Err(format!("{host} has a part starting or ending with a dash"));
        }
    }

    // An all-numeric suffix means this is an address, not a name. Blocking an
    // address in the hosts file does nothing at all, so refusing is kinder than
    // accepting it and quietly achieving nothing.
    let suffix = labels[labels.len() - 1];
    if !suffix.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err(format!("{host} is an address, not a site name"));
    }

    Ok(())
}

/// Tidy what someone typed into the bare apex this protocol expects.
///
/// Lives here rather than only in the UI so the app and the helper agree on what
/// "the same site" means. `https://www.Pinterest.com/pin/123` becomes
/// `pinterest.com`. The leading `www.` goes because the helper adds it back —
/// keeping it would produce `www.www.pinterest.com`.
pub fn normalise_host(input: &str) -> String {
    let mut host = input.trim().to_ascii_lowercase();

    for scheme in ["https://", "http://"] {
        if let Some(rest) = host.strip_prefix(scheme) {
            host = rest.to_string();
        }
    }
    // Path, query and fragment are all "everything after the authority".
    for cut in ['/', '?', '#'] {
        if let Some((head, _)) = host.split_once(cut) {
            host = head.to_string();
        }
    }
    // Credentials, then port.
    if let Some((_, rest)) = host.split_once('@') {
        host = rest.to_string();
    }
    if let Some((head, _)) = host.split_once(':') {
        host = head.to_string();
    }
    if let Some(rest) = host.strip_prefix("www.") {
        host = rest.to_string();
    }

    host.trim_matches('.').to_string()
}

/// Refuse a block request old enough that nobody is waiting for it.
///
/// Only `block` is checked. Replaying an unblock is harmless — the lock refuses
/// an early one on its own terms — and the uninstaller writes an emergency
/// unblock by hand with no timestamp at all, which must keep working.
pub fn fresh_enough(request: &Request, now: i64) -> Result<(), String> {
    if request.action != Action::Block {
        return Ok(());
    }
    match request.issued_at {
        None => Err("refused: a block request with no issue time, so it cannot be told \
                     apart from one left over from a previous session"
            .into()),
        Some(issued) if now - issued > MAX_REQUEST_AGE => Err(format!(
            "refused: this block request is {} seconds old",
            now - issued
        )),
        // A timestamp in the future would otherwise never age out, which turns
        // the staleness check off entirely — by a wrong clock or on purpose.
        Some(issued) if issued - now > MAX_REQUEST_AGE => Err(format!(
            "refused: this block request is dated {} seconds in the future",
            issued - now
        )),
        Some(_) => Ok(()),
    }
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
    /// Sites the user named themselves. Recorded so a restart mid-block knows
    /// the full set, and so re-arming cannot quietly drop entries.
    #[serde(default)]
    pub custom_hosts: Vec<String>,
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

/// What Threshold's section of the hosts file actually looks like.
///
/// This used to be a `bool` answered by `contents.contains(HOSTS_BEGIN)`, which
/// was wrong in a way that could strand someone permanently. Removal matches the
/// marker line *exactly*; verification matched by substring. A file whose BEGIN
/// line was damaged — a hand edit, a third-party hosts manager, a truncated
/// write — therefore had its `0.0.0.0` lines copied through as "not ours", while
/// the check happily reported no block. The app then said "the sites are open
/// again" and deleted the lock, with every domain still null-routed and nothing
/// left that could ever find or undo it.
///
/// So the states are named instead of collapsed, and `Unreadable` is no longer
/// silently the same answer as `Clean`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostsState {
    /// No block, no leftovers.
    Clean,
    /// An intact block, and the hosts inside it.
    Blocked(Vec<String>),
    /// Entries with no intact marker around them: nothing will ever remove these
    /// on its own, so they have to be surfaced and repaired rather than reported
    /// as an absence of blocking.
    Orphaned(Vec<String>),
    /// We could not tell. Never treat this as "clean" — that is how a momentary
    /// read failure satisfies an unblock confirmation on the first poll.
    Unreadable(String),
}

impl HostsState {
    /// The hosts currently null-routed, whether or not the markers survived.
    pub fn entries(&self) -> &[String] {
        match self {
            HostsState::Blocked(hosts) | HostsState::Orphaned(hosts) => hosts,
            _ => &[],
        }
    }
}

const NULL_ROUTE: &str = "0.0.0.0 ";

/// Pure so every shape of damage is a test rather than a claim.
pub fn parse_hosts(contents: &str) -> HostsState {
    let entry = |line: &str| -> Option<String> {
        line.trim()
            .strip_prefix(NULL_ROUTE)
            .map(|host| host.trim().to_string())
            .filter(|host| !host.is_empty())
    };

    let mut inside = false;
    let mut has_begin = false;
    let mut has_end = false;
    let mut damaged_marker = false;
    let mut blocked = Vec::new();
    let mut all_entries = Vec::new();

    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed == HOSTS_BEGIN {
            has_begin = true;
            inside = true;
            continue;
        }
        if trimmed == HOSTS_END {
            has_end = true;
            inside = false;
            continue;
        }
        // A marker that no longer matches exactly is the dangerous case: the
        // remover will not recognise it either.
        if trimmed.contains("THRESHOLD") {
            damaged_marker = true;
        }
        if let Some(host) = entry(trimmed) {
            if inside {
                blocked.push(host.clone());
            }
            all_entries.push(host);
        }
    }

    if has_begin {
        return HostsState::Blocked(blocked);
    }
    if has_end || damaged_marker {
        return HostsState::Orphaned(all_entries);
    }
    HostsState::Clean
}

pub fn hosts_state() -> HostsState {
    match std::fs::read_to_string(system_hosts()) {
        Ok(contents) => parse_hosts(&contents),
        // A missing hosts file is unusual but genuinely means nothing is blocked.
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => HostsState::Clean,
        Err(err) => HostsState::Unreadable(err.to_string()),
    }
}

/// Does the hosts file currently carry Threshold's block?
///
/// Kept as the cheap question for callers that only need a yes. Orphaned entries
/// count as blocked, because from the browser's point of view they are.
pub fn hosts_has_block() -> bool {
    matches!(
        hosts_state(),
        HostsState::Blocked(_) | HostsState::Orphaned(_)
    )
}

/// Is the hosts file confirmed clear? Distinct from `!hosts_has_block()`, which
/// would also be true when we could not read the file at all.
pub fn hosts_confirmed_clear() -> bool {
    matches!(hosts_state(), HostsState::Clean)
}

/// Which of `wanted` did not make it into the hosts file.
///
/// This is what catches a helper too old to understand a field the app has
/// started sending: `#[serde(default)]` means it accepts the request and blocks
/// only the part it recognises, and without this the app would report success.
pub fn hosts_missing(wanted: &[String]) -> Vec<String> {
    let state = hosts_state();
    let present = state.entries();
    wanted
        .iter()
        .filter(|host| !present.iter().any(|got| got == *host))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block() -> Request {
        Request::block(vec!["social".into()], vec![], 1_800_000_000, 1_799_999_000)
    }

    #[test]
    fn a_block_may_name_only_custom_sites() {
        let mut request = block();
        request.categories.clear();
        request.custom_hosts = vec!["pinterest.com".into()];
        assert!(validate(&request).is_ok());
    }

    #[test]
    fn a_block_naming_nothing_at_all_is_refused() {
        let mut request = block();
        request.categories.clear();
        assert!(validate(&request).is_err());
    }

    #[test]
    fn accepts_ordinary_site_names() {
        for host in [
            "pinterest.com",
            "news.ycombinator.com",
            "x.com",
            "a-b.co.uk",
            "9gag.com",
        ] {
            assert!(valid_hostname(host).is_ok(), "{host} should be accepted");
        }
    }

    #[test]
    fn refuses_anything_that_is_not_a_plain_hostname() {
        for host in [
            "",
            "localhost",
            "1.2.3.4",
            "192.168.0.1",
            "http://foo.com",
            "foo.com/path",
            "foo.com:8080",
            "a b.com",
            "Foo.com",
            "-foo.com",
            "foo-.com",
            "foo..com",
            "../../windows",
            "foo.com\n0.0.0.0 bank.com",
        ] {
            assert!(valid_hostname(host).is_err(), "{host} should be refused");
        }
    }

    #[test]
    fn a_name_longer_than_dns_allows_is_refused() {
        let long = format!("{}.com", "a".repeat(250));
        assert!(valid_hostname(&long).is_err());
    }

    #[test]
    fn normalising_reduces_a_pasted_url_to_its_apex() {
        assert_eq!(normalise_host("  https://www.Pinterest.com/pin/123  "), "pinterest.com");
        assert_eq!(normalise_host("http://x.com"), "x.com");
        assert_eq!(normalise_host("news.ycombinator.com"), "news.ycombinator.com");
        assert_eq!(normalise_host("foo.com:8080"), "foo.com");
        assert_eq!(normalise_host("user@foo.com"), "foo.com");
        assert_eq!(normalise_host("foo.com?a=b"), "foo.com");
    }

    #[test]
    fn normalising_then_validating_accepts_what_a_person_would_type() {
        for typed in ["WWW.Reddit.com/r/rust", "https://x.com/", "pinterest.com "] {
            let host = normalise_host(typed);
            assert!(valid_hostname(&host).is_ok(), "{typed} -> {host}");
        }
    }

    #[test]
    fn too_many_custom_sites_is_refused() {
        let mut request = block();
        request.custom_hosts = (0..MAX_CUSTOM_HOSTS + 1)
            .map(|n| format!("site{n}.com"))
            .collect();
        assert!(validate(&request).is_err());
    }

    #[test]
    fn a_stale_block_request_is_refused_but_an_unblock_is_not() {
        let request = block();
        let issued = request.issued_at.expect("block carries an issue time");
        assert!(fresh_enough(&request, issued + 5).is_ok());
        assert!(fresh_enough(&request, issued + MAX_REQUEST_AGE + 1).is_err());
        // A future timestamp would never age out, disabling the check entirely.
        assert!(fresh_enough(&request, issued - MAX_REQUEST_AGE - 1).is_err());

        // The uninstaller writes this one by hand, with no timestamp.
        assert!(fresh_enough(&Request::emergency_unblock(), 0).is_ok());
        assert!(fresh_enough(&Request::unblock(), 0).is_ok());
    }

    #[test]
    fn a_block_request_with_no_issue_time_is_refused() {
        let mut request = block();
        request.issued_at = None;
        assert!(fresh_enough(&request, 1_800_000_000).is_err());
    }

    const PLAIN: &str = "127.0.0.1 localhost\n";

    #[test]
    fn an_intact_block_reports_its_entries() {
        let file = format!("{PLAIN}{HOSTS_BEGIN}\n0.0.0.0 x.com\n0.0.0.0 www.x.com\n{HOSTS_END}\n");
        assert_eq!(
            parse_hosts(&file),
            HostsState::Blocked(vec!["x.com".into(), "www.x.com".into()])
        );
    }

    #[test]
    fn a_clean_file_is_clean() {
        assert_eq!(parse_hosts(PLAIN), HostsState::Clean);
        assert_eq!(parse_hosts(""), HostsState::Clean);
    }

    /// The bug this whole type exists for. The old check asked
    /// `contents.contains(HOSTS_BEGIN)`; removal asks for an exact line. A
    /// damaged BEGIN satisfies neither the remover nor the old check, so the app
    /// reported "the sites are open again" while every entry stayed live.
    #[test]
    fn entries_without_an_intact_marker_are_never_reported_as_clean() {
        let damaged = format!(
            "{PLAIN}# THRESHOLD-BEGIN armed 13:13\n0.0.0.0 x.com\n0.0.0.0 facebook.com\n{HOSTS_END}\n"
        );
        match parse_hosts(&damaged) {
            HostsState::Orphaned(hosts) => {
                assert!(hosts.contains(&"x.com".to_string()));
                assert!(hosts.contains(&"facebook.com".to_string()));
            }
            other => panic!("expected orphaned entries, got {other:?}"),
        }
    }

    #[test]
    fn an_end_marker_with_no_beginning_is_orphaned_too() {
        let damaged = format!("{PLAIN}0.0.0.0 x.com\n{HOSTS_END}\n");
        assert!(matches!(parse_hosts(&damaged), HostsState::Orphaned(_)));
    }

    #[test]
    fn orphaned_entries_still_count_as_blocked_because_the_browser_thinks_so() {
        let state = parse_hosts(&format!("{PLAIN}0.0.0.0 x.com\n{HOSTS_END}\n"));
        assert_eq!(state.entries(), ["x.com".to_string()]);
    }

    #[test]
    fn a_null_route_outside_our_markers_is_left_alone() {
        // Somebody else's blocklist is not ours to report or to remove.
        assert_eq!(parse_hosts("0.0.0.0 someone-elses.example\n"), HostsState::Clean);
    }

    #[test]
    fn the_documented_wire_shape_still_parses_without_the_new_fields() {
        let raw = r#"{"action":"block","categories":["social"],"until":1800000000}"#;
        let request: Request = serde_json::from_str(raw).expect("parses");
        assert!(request.custom_hosts.is_empty());
        assert_eq!(request.issued_at, None);
    }
}
