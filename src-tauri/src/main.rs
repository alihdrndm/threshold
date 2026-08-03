// Threshold lives in the tray; a console window flashing at logon would defeat
// the point of a calm interceptor.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Task registration is a maintenance command, not an app launch.
    if threshold_lib::handle_task_cli(&args) {
        return;
    }

    threshold_lib::run()
}
