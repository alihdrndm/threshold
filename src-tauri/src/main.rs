// Threshold lives in the tray; a console window flashing at logon would defeat
// the point of a calm interceptor.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Task registration is a maintenance command, not an app launch.
    if args.len() > 1 {
        attach_console();
        if threshold_lib::handle_task_cli(&args) {
            return;
        }
    }

    threshold_lib::run()
}

/// Borrow the terminal that launched us, so command-line output is visible.
///
/// The release build is a GUI subsystem binary — deliberately, so no console
/// flashes at logon — which means `println!` goes nowhere. Every diagnostic
/// command printed to a void in the shipped app, which is a poor joke for
/// commands whose whole job is telling you what is wrong.
#[cfg(windows)]
fn attach_console() {
    use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
    // Failure just means there is no parent console (launched from Explorer),
    // in which case there is nothing to attach to and nothing to report.
    let _ = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
}

#[cfg(not(windows))]
fn attach_console() {}
