//! Local SQLite store. There is no server and never will be — the whole point
//! is that nothing about how you spend your attention leaves the machine.
//!
//! Accessed from Rust rather than through a JS-facing SQL plugin, because
//! streak counting, lock expiry and debounce all need the database at moments
//! when no webview is alive.

pub mod intentions;

use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::Connection;

pub struct Db(pub Mutex<Connection>);

pub fn data_dir() -> Result<PathBuf, String> {
    let dir = dirs::data_dir()
        .ok_or_else(|| "no roaming application data directory".to_string())?
        .join("Threshold");
    std::fs::create_dir_all(&dir).map_err(|err| format!("could not create {dir:?}: {err}"))?;
    Ok(dir)
}

pub fn open() -> Result<Connection, String> {
    let path = data_dir()?.join("threshold.db");
    let conn = Connection::open(&path).map_err(|err| format!("could not open {path:?}: {err}"))?;
    migrate(&conn)?;
    Ok(conn)
}

/// Migrations are applied by `user_version`, so adding a table later is a new
/// numbered step rather than an edit to an old one.
fn migrate(conn: &Connection) -> Result<(), String> {
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")
        .map_err(|err| format!("could not configure database: {err}"))?;

    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|err| format!("could not read schema version: {err}"))?;

    if version < 1 {
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS intentions(
                id INTEGER PRIMARY KEY,
                ts TEXT NOT NULL,
                text TEXT,
                if_then TEXT,
                predicted_yes INTEGER,
                duration_min INTEGER,
                categories TEXT,
                trigger TEXT,
                outcome TEXT CHECK(outcome IN ('completed','skipped','drifted','browsing'))
            );
            CREATE TABLE IF NOT EXISTS drift_events(
                id INTEGER PRIMARY KEY,
                ts TEXT NOT NULL,
                note TEXT
            );
            CREATE TABLE IF NOT EXISTS settings(
                key TEXT PRIMARY KEY,
                value TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_intentions_ts ON intentions(ts);
            PRAGMA user_version = 1;
            "#,
        )
        .map_err(|err| format!("migration 1 failed: {err}"))?;
    }

    Ok(())
}

#[cfg(test)]
pub(crate) fn migrate_for_tests(conn: &Connection) {
    migrate(conn).expect("test schema");
}

pub fn get_setting(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
        row.get::<_, String>(0)
    })
    .ok()
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO settings(key, value) VALUES(?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [key, value],
    )
    .map(|_| ())
    .map_err(|err| format!("could not save {key}: {err}"))
}
