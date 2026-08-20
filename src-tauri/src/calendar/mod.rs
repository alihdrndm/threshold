//! Google Calendar for the Schedule quadrant.
//!
//! When a task enters Schedule it gets a calendar event at the next free slot;
//! moving or leaving the quadrant moves or removes the event; and a background
//! poll reflects changes the user makes in Google back onto the card. The one
//! piece of judgement - which slot - lives in `slot`, pure and tested. The rest
//! is plumbing: OAuth, the HTTP calls, and the thread that keeps them off the
//! main loop.
//!
//! Nothing here ever blocks a task move. If the app is not connected or a call
//! fails, the task still lands in Schedule with no date, the card says so, and
//! the error goes to the line the dashboard already shows. A calendar is a
//! convenience on top of the matrix, never a gate in front of it.

pub mod api;
pub mod oauth;
pub mod repeat;
pub mod settings;
pub mod slot;
pub mod sync;
pub mod token;
pub mod week;

use tauri::AppHandle;

/// Start the background reconciler. Called once from `setup()`.
pub fn init(app: &AppHandle) {
    sync::spawn_poller(app.clone());
}
