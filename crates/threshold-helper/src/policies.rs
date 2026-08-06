//! Blocking at a layer the browser cannot cache its way around.
//!
//! Two jobs, and the second one is why the engine works at all.
//!
//! **Closing the DNS-over-HTTPS bypass.** Chrome and Edge resolve names over
//! HTTPS to their own resolver and never consult the hosts file, so without this
//! every entry we write is simply ignored.
//!
//! **Blocking the URL itself.** The hosts file sits *underneath* the browser,
//! and things live above it. A site with a service worker — x.com is one —
//! answers a navigation from its own cache before any name is resolved, so a
//! null route never comes into it. The same gap runs the other way: when a block
//! lifts, `ipconfig /flushdns` clears Windows' cache and cannot touch the
//! browser's in-process resolver cache, its warm sockets, or its copy of these
//! very policies, which is why unblocking used to appear not to work.
//! `URLBlocklist` is enforced in the navigation path, above all of that, and a
//! single apex entry covers every subdomain — so subdomains stop needing a
//! hand-maintained list.
//!
//! The cost is visible and worth stating plainly in the UI: while a block is
//! armed the browsers will report that they are "managed by your organization",
//! and a blocked site shows the browser's own "blocked by your administrator"
//! page rather than a connection error.
//!
//! These keys are removed on full unblock, and the uninstaller removes them too.
//! That matters more here than it did for DoH alone: a stranded `URLBlocklist`
//! is invisible, survives a reinstall, and cannot be fixed with Notepad. So
//! removal is verified by reading back, never assumed.

use std::collections::BTreeMap;

use windows::core::PCWSTR;
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteKeyW, RegDeleteValueW, RegOpenKeyExW, RegQueryInfoKeyW,
    RegQueryValueExW, RegSetValueExW, HKEY, HKEY_LOCAL_MACHINE, KEY_READ, KEY_SET_VALUE, REG_DWORD,
    REG_OPTION_NON_VOLATILE, REG_SZ, REG_VALUE_TYPE,
};

#[derive(Debug, Clone, Copy)]
pub enum Value {
    /// REG_SZ "off" — Chrome and Edge.
    Off,
    /// REG_DWORD 0 plus a Locked flag — Firefox.
    DisabledAndLocked,
}

#[derive(Debug, Clone, Copy)]
pub struct Policy {
    pub browser: &'static str,
    pub path: &'static str,
    pub name: &'static str,
    pub value: Value,
}

/// Never delete at or above this path, however empty it looks.
pub const POLICIES_ROOT: &str = r"SOFTWARE\Policies";

pub const POLICIES: &[Policy] = &[
    Policy {
        browser: "Chrome",
        path: r"SOFTWARE\Policies\Google\Chrome",
        name: "DnsOverHttpsMode",
        value: Value::Off,
    },
    Policy {
        browser: "Edge",
        path: r"SOFTWARE\Policies\Microsoft\Edge",
        name: "DnsOverHttpsMode",
        value: Value::Off,
    },
    Policy {
        browser: "Firefox",
        path: r"SOFTWARE\Policies\Mozilla\Firefox\DNSOverHTTPS",
        name: "Enabled",
        value: Value::DisabledAndLocked,
    },
];

/// How a browser wants a blocked site spelled.
#[derive(Debug, Clone, Copy)]
pub enum PatternStyle {
    /// Chromium: a bare domain blocks it and everything under it.
    Domain,
    /// Firefox: a WebExtension match pattern, where `*.example.com` covers the
    /// apex as well as its subdomains.
    MatchPattern,
}

#[derive(Debug, Clone, Copy)]
pub struct Blocklist {
    pub browser: &'static str,
    /// The subkey holding numbered values `1`, `2`, `3`…
    pub path: &'static str,
    pub style: PatternStyle,
}

/// Brave and Vivaldi are here because they were previously uncovered entirely:
/// no DoH policy, no blocklist, so they walked straight past the whole engine.
/// Opera is deliberately absent — its policy path is not reliably documented,
/// and a key that does nothing is worse than an honest gap.
pub const BLOCKLISTS: &[Blocklist] = &[
    Blocklist {
        browser: "Chrome",
        path: r"SOFTWARE\Policies\Google\Chrome\URLBlocklist",
        style: PatternStyle::Domain,
    },
    Blocklist {
        browser: "Edge",
        path: r"SOFTWARE\Policies\Microsoft\Edge\URLBlocklist",
        style: PatternStyle::Domain,
    },
    Blocklist {
        browser: "Brave",
        path: r"SOFTWARE\Policies\BraveSoftware\Brave\URLBlocklist",
        style: PatternStyle::Domain,
    },
    Blocklist {
        browser: "Vivaldi",
        path: r"SOFTWARE\Policies\Vivaldi\URLBlocklist",
        style: PatternStyle::Domain,
    },
    Blocklist {
        browser: "Firefox",
        path: r"SOFTWARE\Policies\Mozilla\Firefox\WebsiteFilter\Block",
        style: PatternStyle::MatchPattern,
    },
];

/// Chromium's documented ceiling for a list policy.
pub const MAX_LIST: u32 = 1000;

pub fn pattern_for(apex: &str, style: PatternStyle) -> String {
    match style {
        PatternStyle::Domain => apex.to_string(),
        PatternStyle::MatchPattern => format!("*://*.{apex}/*"),
    }
}

/// Human-readable description of what applying these would do, for the dry run.
pub fn describe(apexes: &[String]) -> Vec<String> {
    let mut lines = describe_doh();
    for list in BLOCKLISTS {
        let sample = apexes
            .first()
            .map(|apex| pattern_for(apex, list.style))
            .unwrap_or_else(|| "(nothing)".into());
        lines.push(format!(
            "HKLM\\{}\\1..{} = \"{sample}\", …   [{}]",
            list.path,
            apexes.len(),
            list.browser
        ));
    }
    lines
}

fn describe_doh() -> Vec<String> {
    POLICIES
        .iter()
        .map(|policy| match policy.value {
            Value::Off => format!(
                "HKLM\\{}\\{} = \"off\" (REG_SZ)   [{}]",
                policy.path, policy.name, policy.browser
            ),
            Value::DisabledAndLocked => format!(
                "HKLM\\{}\\{} = 0 (REG_DWORD), Locked = 1   [{}]",
                policy.path, policy.name, policy.browser
            ),
        })
        .collect()
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Open for writing, creating the key if it does not exist.
fn open_write(path: &str) -> Result<HKEY, String> {
    let mut key = HKEY::default();
    let wide_path = wide(path);
    let status = unsafe {
        RegCreateKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(wide_path.as_ptr()),
            None,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut key,
            None,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(format!("could not open HKLM\\{path}: {status:?}"));
    }
    Ok(key)
}

/// Open only if it already exists.
///
/// Removal must never use the creating variant: it would conjure the very keys
/// it is about to delete, so a machine that had never met Threshold would come
/// away with policy keys it never had.
fn open_existing(path: &str, access: windows::Win32::System::Registry::REG_SAM_FLAGS) -> Option<HKEY> {
    let mut key = HKEY::default();
    let wide_path = wide(path);
    let status = unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(wide_path.as_ptr()),
            None,
            access,
            &mut key,
        )
    };
    (status == ERROR_SUCCESS).then_some(key)
}

fn read_string(key: HKEY, name: &str) -> Option<String> {
    let wide_name = wide(name);
    let mut kind = REG_VALUE_TYPE::default();
    let mut size = 0u32;

    let status = unsafe {
        RegQueryValueExW(
            key,
            PCWSTR(wide_name.as_ptr()),
            None,
            Some(&mut kind),
            None,
            Some(&mut size),
        )
    };
    if status != ERROR_SUCCESS || size == 0 {
        return None;
    }

    let mut buffer = vec![0u8; size as usize];
    let status = unsafe {
        RegQueryValueExW(
            key,
            PCWSTR(wide_name.as_ptr()),
            None,
            Some(&mut kind),
            Some(buffer.as_mut_ptr()),
            Some(&mut size),
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }

    let units: Vec<u16> = buffer
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    Some(
        String::from_utf16_lossy(&units)
            .trim_end_matches('\0')
            .to_string(),
    )
}

/// Read a numbered list policy the way the browser reads it.
///
/// Browsers walk `1`, `2`, `3`… and stop at the first missing index, so a gap
/// silently truncates the list. Reading by the same rule is what keeps
/// `write_list` honest about renumbering.
fn read_list(path: &str) -> Vec<String> {
    let Some(key) = open_existing(path, KEY_READ) else {
        return Vec::new();
    };
    let mut values = Vec::new();
    for index in 1..=MAX_LIST {
        match read_string(key, &index.to_string()) {
            Some(value) => values.push(value),
            None => break,
        }
    }
    let _ = unsafe { RegCloseKey(key) };
    values
}

/// Write a numbered list, contiguously, and clear anything left over.
fn write_list(path: &str, values: &[String]) -> Result<(), String> {
    let key = open_write(path)?;
    let mut outcome = Ok(());

    for (index, value) in values.iter().enumerate() {
        if let Err(err) = set_string(key, &(index + 1).to_string(), value) {
            outcome = Err(err);
            break;
        }
    }

    // A longer previous list must not leave a tail behind: the browser would
    // keep enforcing entries nobody asked for.
    if outcome.is_ok() {
        for index in (values.len() as u32 + 1)..=MAX_LIST {
            let name = wide(&index.to_string());
            if unsafe { RegDeleteValueW(key, PCWSTR(name.as_ptr())) } != ERROR_SUCCESS {
                break;
            }
        }
    }

    let _ = unsafe { RegCloseKey(key) };
    outcome
}

fn set_string(key: HKEY, name: &str, value: &str) -> Result<(), String> {
    let wide_name = wide(name);
    let wide_value = wide(value);
    let bytes = unsafe {
        std::slice::from_raw_parts(
            wide_value.as_ptr() as *const u8,
            wide_value.len() * std::mem::size_of::<u16>(),
        )
    };
    let status =
        unsafe { RegSetValueExW(key, PCWSTR(wide_name.as_ptr()), None, REG_SZ, Some(bytes)) };
    if status != ERROR_SUCCESS {
        return Err(format!("could not set {name}: {status:?}"));
    }
    Ok(())
}

fn set_dword(key: HKEY, name: &str, value: u32) -> Result<(), String> {
    let wide_name = wide(name);
    let bytes = value.to_le_bytes();
    let status = unsafe {
        RegSetValueExW(
            key,
            PCWSTR(wide_name.as_ptr()),
            None,
            REG_DWORD,
            Some(&bytes),
        )
    };
    if status != ERROR_SUCCESS {
        return Err(format!("could not set {name}: {status:?}"));
    }
    Ok(())
}

/// Whatever was in the blocklist keys before Threshold ever touched them.
///
/// These keys are shared ground: an organisation may already manage `URLBlocklist`
/// on this machine, and destroying their entries would be indefensible. So the
/// original is snapshotted into the admin-only state directory on the first arm
/// and restored verbatim on unblock.
fn backup_path() -> std::path::PathBuf {
    threshold_protocol::policy_backup_path()
}

fn load_backup() -> Option<BTreeMap<String, Vec<String>>> {
    let raw = std::fs::read_to_string(backup_path()).ok()?;
    threshold_protocol::parse_json(&raw).ok()
}

fn save_backup(snapshot: &BTreeMap<String, Vec<String>>) -> Result<(), String> {
    let dir = threshold_protocol::state_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|err| format!("could not create {}: {err}", dir.display()))?;
    let body = serde_json::to_string_pretty(snapshot)
        .map_err(|err| format!("could not encode the policy backup: {err}"))?;
    std::fs::write(backup_path(), body)
        .map_err(|err| format!("could not write the policy backup: {err}"))
}

/// Block these apexes in every browser we can reach.
pub fn apply_blocklist(apexes: &[String]) -> Result<(), String> {
    // Snapshot once and only once. A second arm must not record our own entries
    // as somebody else's, or unblocking would restore the block.
    let original = match load_backup() {
        Some(saved) => saved,
        None => {
            let snapshot: BTreeMap<String, Vec<String>> = BLOCKLISTS
                .iter()
                .map(|list| (list.path.to_string(), read_list(list.path)))
                .collect();
            save_backup(&snapshot)?;
            snapshot
        }
    };

    for list in BLOCKLISTS {
        let mut combined = original.get(list.path).cloned().unwrap_or_default();
        for apex in apexes {
            let pattern = pattern_for(apex, list.style);
            if !combined.contains(&pattern) {
                combined.push(pattern);
            }
        }
        combined.truncate(MAX_LIST as usize);
        write_list(list.path, &combined)?;
    }

    Ok(())
}

/// Put the blocklist keys back exactly as they were found.
///
/// `known` is the fallback for the case where the snapshot is gone but entries
/// are not — a wiped state directory. Without it those entries would be
/// unremovable by any means the product offers, and a `URLBlocklist` cannot be
/// fixed with Notepad the way a hosts file can.
pub fn remove_blocklist(known: &[String]) -> Result<(), String> {
    let backup = load_backup();

    for list in BLOCKLISTS {
        match backup.as_ref().and_then(|saved| saved.get(list.path)) {
            Some(theirs) => {
                if theirs.is_empty() {
                    clear_list(list.path);
                } else {
                    write_list(list.path, theirs)?;
                }
            }
            None => {
                // No snapshot: keep anything we cannot prove is ours.
                let ours: Vec<String> = known
                    .iter()
                    .map(|apex| pattern_for(apex, list.style))
                    .collect();
                let remaining: Vec<String> = read_list(list.path)
                    .into_iter()
                    .filter(|value| !ours.contains(value))
                    .collect();
                if remaining.is_empty() {
                    clear_list(list.path);
                } else {
                    write_list(list.path, &remaining)?;
                }
            }
        }
    }

    // Read back while the backup still exists — it is what tells our entries
    // apart from an organisation's. Only then is it safe to drop.
    let surviving = surviving_blocklist();
    if !surviving.is_empty() {
        return Err(format!(
            "these sites are still blocked by browser policy: {}",
            surviving.join(", ")
        ));
    }

    let _ = std::fs::remove_file(backup_path());
    Ok(())
}

/// Delete every numbered value, then the key itself if nothing else lives there.
fn clear_list(path: &str) {
    if let Some(key) = open_existing(path, KEY_SET_VALUE) {
        for index in 1..=MAX_LIST {
            let name = wide(&index.to_string());
            if unsafe { RegDeleteValueW(key, PCWSTR(name.as_ptr())) } != ERROR_SUCCESS {
                break;
            }
        }
        let _ = unsafe { RegCloseKey(key) };
    }

    let mut path = path;
    loop {
        delete_if_empty(path);
        match path.rsplit_once('\\') {
            Some((parent, _)) if parent.len() > POLICIES_ROOT.len() => path = parent,
            _ => break,
        }
    }
}

pub fn apply() -> Result<(), String> {
    for policy in POLICIES {
        let key = open_write(policy.path)?;
        let result = match policy.value {
            Value::Off => set_string(key, policy.name, "off"),
            Value::DisabledAndLocked => {
                set_dword(key, policy.name, 0).and_then(|()| set_dword(key, "Locked", 1))
            }
        };
        let _ = unsafe { RegCloseKey(key) };
        result?;
    }
    Ok(())
}

/// Delete the key itself, but only when nothing else lives in it.
///
/// These keys are shared ground: an organisation may have its own Chrome
/// policies under the same path. Removing a key that still holds values or
/// subkeys would destroy settings that were never ours.
fn delete_if_empty(path: &str) {
    let wide_path = wide(path);
    let mut key = HKEY::default();
    let opened = unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(wide_path.as_ptr()),
            None,
            KEY_READ,
            &mut key,
        )
    };
    if opened != ERROR_SUCCESS {
        return;
    }

    let mut values = 0u32;
    let mut subkeys = 0u32;
    let status = unsafe {
        RegQueryInfoKeyW(
            key,
            None,
            None,
            None,
            Some(&mut subkeys),
            None,
            None,
            Some(&mut values),
            None,
            None,
            None,
            None,
        )
    };
    let _ = unsafe { RegCloseKey(key) };

    if status == ERROR_SUCCESS && values == 0 && subkeys == 0 {
        let _ = unsafe { RegDeleteKeyW(HKEY_LOCAL_MACHINE, PCWSTR(wide_path.as_ptr())) };
    }
}

/// Remove the DoH policies, and say so only if they are actually gone.
///
/// This used to discard every delete result and return `Ok(())` unconditionally,
/// so a removal that failed outright was indistinguishable from one that worked.
/// The UI then told the user the sites were open again. Anything left behind is
/// now reported by name.
pub fn remove() -> Result<(), String> {
    for policy in POLICIES {
        // A key that never existed is already in the desired state — and note
        // this must not be the creating variant, or removal would conjure the
        // keys it is about to delete.
        let Some(key) = open_existing(policy.path, KEY_SET_VALUE) else {
            continue;
        };
        // A value that was never set is already in the desired state, so a
        // failed delete is not on its own an error; the read-back below decides.
        let wide_name = wide(policy.name);
        let _ = unsafe { RegDeleteValueW(key, PCWSTR(wide_name.as_ptr())) };
        if matches!(policy.value, Value::DisabledAndLocked) {
            let locked = wide("Locked");
            let _ = unsafe { RegDeleteValueW(key, PCWSTR(locked.as_ptr())) };
        }
        let _ = unsafe { RegCloseKey(key) };

        // Leave no empty shell behind: a machine that has finished with
        // Threshold should look like it never met it. Creating a nested key
        // creates its parents too, so those get the same treatment - each still
        // guarded by "only if empty", and never above SOFTWARE\Policies.
        let mut path = policy.path;
        loop {
            delete_if_empty(path);
            match path.rsplit_once('\\') {
                Some((parent, _)) if parent.len() > POLICIES_ROOT.len() => path = parent,
                _ => break,
            }
        }
    }

    let surviving = surviving_doh();
    if surviving.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "these policy settings could not be removed: {}",
            surviving.join(", ")
        ))
    }
}

fn surviving_doh() -> Vec<String> {
    let mut found = Vec::new();
    for policy in POLICIES {
        if let Some(key) = open_existing(policy.path, KEY_READ) {
            if read_string(key, policy.name).is_some() {
                found.push(format!("HKLM\\{}\\{}", policy.path, policy.name));
            }
            let _ = unsafe { RegCloseKey(key) };
        }
    }
    found
}

/// Blocked sites still in force that this app put there.
///
/// The backup file's existence is what marks these keys as ours. Once removal
/// has restored the original and deleted the backup, whatever remains belongs to
/// somebody else — an organisation's own blocklist is not a fault to report.
fn surviving_blocklist() -> Vec<String> {
    let Some(backup) = load_backup() else {
        return Vec::new();
    };

    let mut found = Vec::new();
    for list in BLOCKLISTS {
        let theirs = backup.get(list.path).cloned().unwrap_or_default();
        let extra = read_list(list.path)
            .into_iter()
            .filter(|value| !theirs.contains(value))
            .count();
        if extra > 0 {
            found.push(format!("HKLM\\{} ({extra} sites)", list.path));
        }
    }
    found
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn covers_every_browser_that_can_bypass_the_hosts_file() {
        let browsers: Vec<_> = POLICIES.iter().map(|p| p.browser).collect();
        assert!(browsers.contains(&"Chrome"));
        assert!(browsers.contains(&"Edge"));
        assert!(browsers.contains(&"Firefox"));
    }

    /// Mirrors the walk in `remove()` so the stopping rule is testable without
    /// touching the real registry.
    fn parents_walked(path: &str) -> Vec<String> {
        let mut visited = vec![path.to_string()];
        let mut current = path;
        loop {
            match current.rsplit_once('\\') {
                Some((parent, _)) if parent.len() > POLICIES_ROOT.len() => {
                    visited.push(parent.to_string());
                    current = parent;
                }
                _ => break,
            }
        }
        visited
    }

    #[test]
    fn cleanup_walks_up_to_but_never_past_the_policies_root() {
        let walked = parents_walked(r"SOFTWARE\Policies\Mozilla\Firefox\DNSOverHTTPS");
        assert!(walked.contains(&r"SOFTWARE\Policies\Mozilla\Firefox".to_string()));
        assert!(walked.contains(&r"SOFTWARE\Policies\Mozilla".to_string()));
        assert!(
            !walked.contains(&POLICIES_ROOT.to_string()),
            "must never consider deleting SOFTWARE\\Policies itself"
        );
        assert!(!walked.contains(&"SOFTWARE".to_string()));
    }

    #[test]
    fn description_names_the_exact_keys_and_values() {
        let described = describe(&["x.com".to_string()]).join("\n");
        assert!(described.contains(r"SOFTWARE\Policies\Google\Chrome\DnsOverHttpsMode"));
        assert!(described.contains("\"off\""));
        assert!(described.contains("Locked = 1"));
        assert!(described.contains(r"SOFTWARE\Policies\Google\Chrome\URLBlocklist"));
    }

    /// The whole reason policy blocking is here: x.com's service worker answers
    /// a navigation before DNS is consulted, so one apex has to cover the lot.
    #[test]
    fn a_chromium_pattern_is_the_bare_apex_so_it_covers_every_subdomain() {
        assert_eq!(pattern_for("x.com", PatternStyle::Domain), "x.com");
    }

    #[test]
    fn a_firefox_pattern_covers_the_apex_and_its_subdomains() {
        assert_eq!(
            pattern_for("x.com", PatternStyle::MatchPattern),
            "*://*.x.com/*"
        );
    }

    #[test]
    fn every_browser_that_can_bypass_the_hosts_file_has_a_blocklist_too() {
        let browsers: Vec<_> = BLOCKLISTS.iter().map(|list| list.browser).collect();
        for browser in ["Chrome", "Edge", "Firefox", "Brave", "Vivaldi"] {
            assert!(browsers.contains(&browser), "{browser} is uncovered");
        }
    }

    #[test]
    fn no_blocklist_key_sits_at_or_above_the_policies_root() {
        // Removal walks parents deleting empty keys; a path this shallow would
        // put SOFTWARE\Policies itself in range.
        for list in BLOCKLISTS {
            assert!(
                list.path.len() > POLICIES_ROOT.len()
                    && list.path.starts_with(POLICIES_ROOT),
                "{} is not safely below the policies root",
                list.path
            );
        }
    }
}
