//! Closing the DNS-over-HTTPS bypass.
//!
//! Without this the whole blocking engine is theatre: Chrome and Edge resolve
//! names over HTTPS to their own resolver and never consult the hosts file, so
//! every entry we write is simply ignored.
//!
//! The cost is visible and worth stating plainly in the UI: while a block is
//! armed the browsers will report that they are "managed by your organization".
//! These keys are removed on full unblock, and the uninstaller removes them
//! too — leaving a machine with orphaned policy keys is not acceptable.

use windows::core::PCWSTR;
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteKeyW, RegDeleteValueW, RegOpenKeyExW, RegQueryInfoKeyW,
    RegSetValueExW, HKEY, HKEY_LOCAL_MACHINE, KEY_READ, KEY_SET_VALUE, REG_DWORD,
    REG_OPTION_NON_VOLATILE, REG_SZ,
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

/// Human-readable description of what applying these would do, for the dry run.
pub fn describe() -> Vec<String> {
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

fn open(path: &str) -> Result<HKEY, String> {
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

pub fn apply() -> Result<(), String> {
    for policy in POLICIES {
        let key = open(policy.path)?;
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

pub fn remove() -> Result<(), String> {
    for policy in POLICIES {
        // A key that never existed is already in the desired state.
        let Ok(key) = open(policy.path) else { continue };
        // A value that was never set is already in the desired state, so a
        // failed delete is not an error worth surfacing.
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
    Ok(())
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
        let described = describe().join("\n");
        assert!(described.contains(r"SOFTWARE\Policies\Google\Chrome\DnsOverHttpsMode"));
        assert!(described.contains("\"off\""));
        assert!(described.contains("Locked = 1"));
    }
}
