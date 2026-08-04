//! The IPC surface. Kept thin on purpose: the popup should be able to describe
//! what happened in one call and then get out of the way.

use tauri::{Manager, State};

use crate::db::{self, intentions, tasks, Db};

#[tauri::command]
pub fn ping() -> String {
    "Core responded.".to_string()
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RitualResult {
    pub id: i64,
    /// Present when the session asked for sites to be blocked. `blocked: false`
    /// carries a reason the confirmation screen shows, because telling someone
    /// their sites are blocked when they are not is worse than not blocking.
    pub block: Option<crate::session::BlockOutcome>,
}

/// Record what the user did, arm any block they committed to, then close.
///
/// The record is written first so that a failure further along loses the block,
/// never the answer. Arming happens before the window closes so the outcome can
/// still be shown on the confirmation screen.
#[tauri::command]
pub fn finish_ritual(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    intention: intentions::NewIntention,
) -> Result<RitualResult, String> {
    let id = {
        let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
        intentions::insert(&conn, &intention)?
    };

    let categories: Vec<String> = intention
        .categories
        .as_deref()
        .unwrap_or_default()
        .split(',')
        .filter(|c| !c.is_empty())
        .map(str::to_owned)
        .collect();

    let block = match (categories.is_empty(), intention.duration_min) {
        (false, Some(minutes)) if minutes > 0 => {
            let outcome = crate::session::arm(&categories, minutes);
            if outcome.blocked {
                // Tell the debounce a session is running so the ritual does not
                // reappear on the next wake in the middle of one.
                crate::triggers::note_session(minutes);
            }
            Some(outcome)
        }
        _ => None,
    };

    // A failed block keeps the window open so the reason is read rather than
    // flashing past on the way out.
    let keep_open = block.as_ref().is_some_and(|outcome| !outcome.blocked);
    if !keep_open {
        crate::popup::close(&app).map_err(|err| err.to_string())?;
    }

    Ok(RitualResult { id, block })
}

/// Chips offered on the intention step.
///
/// Sourced from the "Do First" quadrant, which is the entire point of pairing
/// the list with the ritual: at the vulnerable moment you are shown what
/// matters instead of being asked to remember it. Recent intentions fill any
/// remaining slots so the step is never empty on a fresh install.
#[tauri::command]
pub fn intention_suggestions(db: State<'_, Db>) -> Result<Vec<String>, String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;

    let mut chips = tasks::do_first_titles(&conn, 3)?;
    if chips.len() < 3 {
        for text in intentions::recent_texts(&conn, 3)? {
            if chips.len() >= 3 {
                break;
            }
            if !chips.contains(&text) {
                chips.push(text);
            }
        }
    }
    Ok(chips)
}

#[tauri::command]
pub fn list_contexts(db: State<'_, Db>) -> Result<Vec<tasks::Context>, String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    tasks::contexts(&conn)
}

#[tauri::command]
pub fn list_tasks(db: State<'_, Db>) -> Result<Vec<tasks::Task>, String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    tasks::open_tasks(&conn)
}

#[tauri::command]
pub fn add_task(db: State<'_, Db>, task: tasks::NewTask) -> Result<i64, String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    tasks::add(&conn, &task)
}

#[tauri::command]
pub fn move_task(
    db: State<'_, Db>,
    id: i64,
    urgent: Option<bool>,
    important: Option<bool>,
    sort_order: Option<i64>,
) -> Result<(), String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    tasks::set_quadrant(&conn, id, urgent, important, sort_order.unwrap_or(0))
}

#[tauri::command]
pub fn set_task_status(db: State<'_, Db>, id: i64, status: String) -> Result<(), String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    tasks::set_status(&conn, id, &status)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FocusResult {
    pub opened: bool,
    /// Why nothing happened, when nothing happened. Previously this command
    /// returned Ok(()) whether or not it did anything, so a paused app made the
    /// button look broken.
    pub reason: Option<String>,
}

/// Start a focus session from a task, without waiting for a boot or wake.
///
/// Takes the task so the ritual can open with that intention already filled in
/// — otherwise "Focus on this" is indistinguishable from "open the ritual".
#[tauri::command]
pub fn focus_on_task(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    task_id: i64,
) -> Result<FocusResult, String> {
    let title = {
        let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
        tasks::title_of(&conn, task_id)?
    };

    match crate::triggers::request_with_intent(&app, title) {
        Ok(()) => Ok(FocusResult {
            opened: true,
            reason: None,
        }),
        Err(reason) => Ok(FocusResult {
            opened: false,
            reason: Some(reason),
        }),
    }
}

#[tauri::command]
pub fn diagnostics(db: State<'_, Db>) -> Result<crate::diagnostics::Diagnostics, String> {
    let paused = {
        let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
        crate::pause::paused_until(&conn)
    };
    Ok(crate::diagnostics::diagnostics_with_pause(paused))
}

/// Keep the native titlebar in step with the chosen appearance.
///
/// `None` means follow Windows, which also restores the webview's automatic
/// colour scheme — so a "system" preference keeps tracking live OS changes
/// through `prefers-color-scheme`. The two mechanisms are never both in play,
/// which is why they cannot disagree.
#[tauri::command]
pub fn set_window_theme(app: tauri::AppHandle, theme: Option<String>) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let _ = window.set_theme(match theme.as_deref() {
        Some("light") => Some(tauri::Theme::Light),
        Some("dark") => Some(tauri::Theme::Dark),
        _ => None,
    });
}

/// Lift a block that is still owed time, and record that it happened.
///
/// `async` because it waits on the elevated helper for up to twelve seconds;
/// run on the main thread that would freeze every window, including the one
/// showing the button.
///
/// This is the only way out of a commitment before its time, and it exists
/// because the lock deliberately fails closed. It is not hidden and not
/// discouraged: the helper records it neutrally as a count, and an escape hatch
/// people feel judged for using is one they route around instead.
#[tauri::command(async)]
pub fn emergency_unblock() -> Result<(), String> {
    crate::session::emergency_unblock()
}

/// Re-register the elevated helper, prompting for administrator rights.
///
/// The one repair the user cannot perform from inside the app, surfaced as a
/// button rather than left as a command-line incantation.
#[tauri::command]
pub fn repair_helper() -> Result<(), String> {
    crate::elevate_register_helper()
}

#[tauri::command]
pub fn pause_status(db: State<'_, Db>) -> Result<Option<i64>, String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    Ok(crate::pause::paused_until(&conn))
}

#[tauri::command]
pub fn pause_for(db: State<'_, Db>, days: i64) -> Result<i64, String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    crate::pause::pause_for_days(&conn, days)
}

#[tauri::command]
pub fn resume_now(db: State<'_, Db>) -> Result<(), String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    crate::pause::resume(&conn)
}

#[tauri::command]
pub fn get_settings(db: State<'_, Db>) -> Result<Vec<(String, String)>, String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    db::all_settings(&conn)
}

#[tauri::command]
pub fn set_setting(db: State<'_, Db>, key: String, value: String) -> Result<(), String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    db::set_setting(&conn, &key, &value)
}

#[tauri::command]
pub fn recent_intentions(
    db: State<'_, Db>,
    limit: Option<i64>,
) -> Result<Vec<intentions::IntentionRow>, String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    intentions::recent(&conn, limit.unwrap_or(50))
}

/// Block-category toggles are remembered between rituals, because re-choosing
/// them every time is friction that buys nothing.
#[tauri::command]
pub fn remembered_categories(db: State<'_, Db>) -> Result<String, String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    Ok(db::get_setting(&conn, "last_categories").unwrap_or_default())
}

#[tauri::command]
pub fn remember_categories(db: State<'_, Db>, categories: String) -> Result<(), String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    db::set_setting(&conn, "last_categories", &categories)
}

#[tauri::command]
pub fn dismiss_popup(app: tauri::AppHandle) -> Result<(), String> {
    crate::popup::close(&app).map_err(|err| err.to_string())
}

/// Where the database lives, for the dashboard's "your data is here" line.
#[tauri::command]
pub fn data_location() -> Result<String, String> {
    db::data_dir().map(|dir| dir.to_string_lossy().to_string())
}

#[tauri::command]
pub fn open_dashboard(app: tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}
