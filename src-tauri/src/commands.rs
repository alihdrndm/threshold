//! The IPC surface. Kept thin on purpose: the popup should be able to describe
//! what happened in one call and then get out of the way.

use tauri::{Manager, State};

use crate::db::{self, intentions, Db};

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
/// The spec sources these from the "Do First" quadrant; until that exists this
/// is the documented fallback — recent and frequent intentions.
#[tauri::command]
pub fn intention_suggestions(db: State<'_, Db>) -> Result<Vec<String>, String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    intentions::recent_texts(&conn, 3)
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
