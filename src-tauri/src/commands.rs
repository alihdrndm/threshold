//! The IPC surface. Kept thin on purpose: the popup should be able to describe
//! what happened in one call and then get out of the way.

use tauri::{Manager, State};

use crate::db::{self, intentions, tasks, Db};

#[tauri::command]
pub fn ping() -> String {
    "Core responded.".to_string()
}

/// Record what the user did and close the ritual.
///
/// Writing happens before the window closes so that a crash between the two
/// loses the window rather than the record.
#[tauri::command]
pub fn finish_ritual(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    intention: intentions::NewIntention,
) -> Result<i64, String> {
    let id = {
        let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
        intentions::insert(&conn, &intention)?
    };

    crate::popup::close(&app).map_err(|err| err.to_string())?;
    Ok(id)
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

/// Start a focus session from a task, without waiting for a boot or wake.
#[tauri::command]
pub fn focus_on_task(app: tauri::AppHandle) -> Result<(), String> {
    crate::request_ritual(&app);
    Ok(())
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
