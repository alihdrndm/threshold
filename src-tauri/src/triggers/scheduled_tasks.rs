//! Belt-and-suspenders relaunch triggers.
//!
//! The in-process listener misses anything that happens while Threshold is not
//! running, and Modern Standby machines are inconsistent about which resume
//! events they deliver at all. These Task Scheduler entries relaunch the exe on
//! logon, on unlock, and on the Power-Troubleshooter event the kernel writes
//! after every resume. The single-instance plugin makes a relaunch safe: if
//! Threshold is already running, the new process hands its arguments to the
//! live one and exits.
//!
//! These run as the current user with ordinary privileges, so registering them
//! needs no elevation. The one task that *does* need elevation is the blocking
//! helper, which is registered separately during install.
//!
//! Registration is deliberately not automatic — see `main.rs` argument
//! handling. Tasks that point at a development build have a habit of outliving
//! the build.

use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Keep `schtasks` from flashing a console window when we shell out.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub const LOGON_TASK: &str = "ThresholdLogon";
pub const UNLOCK_TASK: &str = "ThresholdUnlock";
pub const RESUME_TASK: &str = "ThresholdResume";

fn schtasks(args: &[&str]) -> Result<String, String> {
    let mut command = Command::new("schtasks.exe");
    command.args(args);

    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);

    let output = command
        .output()
        .map_err(|err| format!("could not run schtasks: {err}"))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

/// `DOMAIN\user` for the current account.
///
/// This is not cosmetic: a logon or session-state trigger with no `UserId`
/// means *every* user on the machine, which Task Scheduler will only let an
/// administrator create. Scoping to one account keeps registration
/// elevation-free, which is the whole point of separating these from the
/// blocking helper.
fn current_user() -> Result<String, String> {
    let user = std::env::var("USERNAME")
        .map_err(|_| "USERNAME is not set in the environment".to_string())?;
    match std::env::var("USERDOMAIN") {
        Ok(domain) if !domain.is_empty() => Ok(format!("{domain}\\{user}")),
        _ => Ok(user),
    }
}

fn exe_path() -> Result<String, String> {
    std::env::current_exe()
        .map_err(|err| format!("could not resolve own path: {err}"))?
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| "executable path is not valid UTF-16 text".to_string())
}

/// XML is the only way to express the unlock and event-log triggers; the plain
/// `schtasks /create /sc` flags cannot describe either one.
fn task_xml(exe: &str, trigger: &str, arg: &str, user: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.4" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>Threshold — reopen the intention prompt when the user returns.</Description>
  </RegistrationInfo>
  <Triggers>
{trigger}
  </Triggers>
  <Principals>
    <Principal id="Author">
      <UserId>{user}</UserId>
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>LeastPrivilege</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <StartWhenAvailable>false</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <IdleSettings>
      <StopOnIdleEnd>false</StopOnIdleEnd>
      <RestartOnIdle>false</RestartOnIdle>
    </IdleSettings>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <RunOnlyIfIdle>false</RunOnlyIfIdle>
    <DisallowStartOnRemoteAppSession>false</DisallowStartOnRemoteAppSession>
    <UseUnifiedSchedulingEngine>true</UseUnifiedSchedulingEngine>
    <WakeToRun>false</WakeToRun>
    <ExecutionTimeLimit>PT1H</ExecutionTimeLimit>
    <Priority>7</Priority>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>"{exe}"</Command>
      <Arguments>{arg}</Arguments>
    </Exec>
  </Actions>
</Task>"#
    )
}

/// A ~10 second delay lets the shell settle first; arriving mid-explorer-start
/// makes the prompt feel like a crash dialog rather than a greeting.
fn logon_trigger(user: &str) -> String {
    format!(
        r#"    <LogonTrigger>
      <Enabled>true</Enabled>
      <UserId>{user}</UserId>
      <Delay>PT10S</Delay>
    </LogonTrigger>"#
    )
}

fn unlock_trigger(user: &str) -> String {
    format!(
        r#"    <SessionStateChangeTrigger>
      <Enabled>true</Enabled>
      <UserId>{user}</UserId>
      <StateChange>SessionUnlock</StateChange>
    </SessionStateChangeTrigger>"#
    )
}

/// Power-Troubleshooter event ID 1 is written by the kernel after every resume,
/// including the Modern Standby wakes that never produce a clean
/// `WM_POWERBROADCAST`.
fn resume_trigger() -> String {
    r#"    <EventTrigger>
      <Enabled>true</Enabled>
      <Subscription>&lt;QueryList&gt;&lt;Query Id="0" Path="System"&gt;&lt;Select Path="System"&gt;*[System[Provider[@Name='Microsoft-Windows-Power-Troubleshooter'] and EventID=1]]&lt;/Select&gt;&lt;/Query&gt;&lt;/QueryList&gt;</Subscription>
    </EventTrigger>"#
        .to_string()
}

fn register_one(name: &str, xml: String) -> Result<(), String> {
    let dir = std::env::temp_dir();
    let path = dir.join(format!("{name}.xml"));

    // Task Scheduler insists on UTF-16LE with a BOM for imported XML.
    let mut bytes = vec![0xFF, 0xFE];
    for unit in xml.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    std::fs::write(&path, bytes).map_err(|err| format!("could not stage {name}: {err}"))?;

    let result = schtasks(&[
        "/create",
        "/tn",
        name,
        "/xml",
        path.to_str().unwrap_or_default(),
        "/f",
    ]);

    let _ = std::fs::remove_file(&path);
    result.map(|_| ())
}

pub fn register_all() -> Result<Vec<String>, String> {
    let exe = exe_path()?;
    let user = current_user()?;
    let mut registered = Vec::new();

    for (name, trigger, arg) in [
        (LOGON_TASK, logon_trigger(&user), "--trigger=boot"),
        (UNLOCK_TASK, unlock_trigger(&user), "--trigger=unlock"),
        (RESUME_TASK, resume_trigger(), "--trigger=wake"),
    ] {
        register_one(name, task_xml(&exe, &trigger, arg, &user))?;
        registered.push(name.to_string());
    }

    Ok(registered)
}

pub fn unregister_all() -> Vec<(String, Result<(), String>)> {
    [LOGON_TASK, UNLOCK_TASK, RESUME_TASK]
        .iter()
        .map(|name| {
            let result = schtasks(&["/delete", "/tn", name, "/f"]).map(|_| ());
            (name.to_string(), result)
        })
        .collect()
}

pub fn status() -> Vec<(String, bool)> {
    [LOGON_TASK, UNLOCK_TASK, RESUME_TASK]
        .iter()
        .map(|name| {
            let exists = schtasks(&["/query", "/tn", name]).is_ok();
            (name.to_string(), exists)
        })
        .collect()
}
