// Threshold lives in the tray; a console window flashing at logon would defeat
// the point of a calm interceptor.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    threshold_lib::run()
}
