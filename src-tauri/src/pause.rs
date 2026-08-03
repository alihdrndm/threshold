//! Pausing Threshold for a few days.
//!
//! A first-class feature, not an admission of failure. Longitudinal data on
//! friction tools shows people take deliberate breaks and come back; a tool
//! that treats a break as defection loses the user permanently at the moment
//! they would otherwise have returned. Supporting the rhythm is what keeps them.
//!
//! When a pause ends the next trigger is a normal ritual, not a lecture.

use rusqlite::Connection;

use crate::db;

const KEY: &str = "paused_until";

/// The active pause, read through the app handle.
pub fn paused_until_for(app: &tauri::AppHandle) -> Option<i64> {
    use tauri::Manager;
    app.try_state::<crate::db::Db>()
        .and_then(|db| db.0.lock().ok().and_then(|conn| paused_until(&conn)))
}

/// Unix seconds the pause runs until, if one is active.
pub fn paused_until(conn: &Connection) -> Option<i64> {
    db::get_setting(conn, KEY)
        .and_then(|raw| raw.parse::<i64>().ok())
        .filter(|until| *until > now())
}

pub fn pause_for_days(conn: &Connection, days: i64) -> Result<i64, String> {
    if !(1..=90).contains(&days) {
        return Err("a pause runs between one and ninety days".into());
    }
    let until = now() + days * 86_400;
    db::set_setting(conn, KEY, &until.to_string())?;
    Ok(until)
}

pub fn resume(conn: &Connection) -> Result<(), String> {
    db::set_setting(conn, KEY, "0")
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory_db() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory database");
        crate::db::migrate_for_tests(&conn);
        conn
    }

    #[test]
    fn nothing_is_paused_by_default() {
        assert!(paused_until(&memory_db()).is_none());
    }

    #[test]
    fn a_pause_is_active_until_it_expires() {
        let conn = memory_db();
        pause_for_days(&conn, 3).unwrap();
        assert!(paused_until(&conn).is_some());
    }

    #[test]
    fn an_expired_pause_reads_as_no_pause() {
        let conn = memory_db();
        db::set_setting(&conn, KEY, &(now() - 10).to_string()).unwrap();
        assert!(paused_until(&conn).is_none());
    }

    #[test]
    fn resuming_ends_the_pause_immediately() {
        let conn = memory_db();
        pause_for_days(&conn, 7).unwrap();
        resume(&conn).unwrap();
        assert!(paused_until(&conn).is_none());
    }

    #[test]
    fn an_absurd_pause_length_is_refused() {
        let conn = memory_db();
        assert!(pause_for_days(&conn, 0).is_err());
        assert!(pause_for_days(&conn, 400).is_err());
    }
}
