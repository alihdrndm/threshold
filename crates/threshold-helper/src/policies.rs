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
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegSetValueExW, HKEY, HKEY_LOCAL_MACHINE,
    KEY_SET_VALUE, REG_DWORD, REG_OPTION_NON_VOLATILE, REG_SZ,
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

    #[test]
    fn description_names_the_exact_keys_and_values() {
        let described = describe().join("\n");
        assert!(described.contains(r"SOFTWARE\Policies\Google\Chrome\DnsOverHttpsMode"));
        assert!(described.contains("\"off\""));
        assert!(described.contains("Locked = 1"));
    }
}
