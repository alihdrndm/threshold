//! Local SQLite store. There is no server and never will be — the whole point
//! is that nothing about how you spend your attention leaves the machine.
//!
//! Accessed from Rust rather than through a JS-facing SQL plugin, because
//! streak counting, lock expiry and debounce all need the database at moments
//! when no webview is alive.

pub mod intentions;
pub mod tasks;

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
///
/// Each step runs inside a transaction, with its `PRAGMA user_version` bump as
/// the last statement, so a step either lands whole or not at all. Without that,
/// a failure partway leaves the schema changed and the version unbumped, and
/// every later launch re-runs a step that can no longer succeed — a duplicate
/// column is enough. `db::open()` failing aborts `setup()`, so the result is an
/// app that does not start at all: no window, no tray, no way to repair it, and
/// possibly a live block with nothing left to lift it.
fn migrate(conn: &Connection) -> Result<(), String> {
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")
        .map_err(|err| format!("could not configure database: {err}"))?;

    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|err| format!("could not read schema version: {err}"))?;

    if version < 1 {
        conn.execute_batch(
            r#"
            BEGIN;
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
            COMMIT;
            "#,
        )
        .map_err(|err| format!("migration 1 failed: {err}"))?;
    }

    if version < 2 {
        // Contexts are data, not code: they are seeded, then renamed and added
        // to from settings.
        conn.execute_batch(
            r#"
            BEGIN;
            CREATE TABLE IF NOT EXISTS contexts(
                id INTEGER PRIMARY KEY,
                name TEXT UNIQUE,
                sort_order INTEGER
            );
            CREATE TABLE IF NOT EXISTS tasks(
                id INTEGER PRIMARY KEY,
                title TEXT NOT NULL,
                note TEXT,
                context_id INTEGER REFERENCES contexts(id),
                urgent INTEGER,
                important INTEGER,
                sort_order INTEGER DEFAULT 0,
                status TEXT DEFAULT 'open'
                    CHECK(status IN ('open','done','archived','deleted')),
                created_ts TEXT,
                completed_ts TEXT
            );
            ALTER TABLE intentions ADD COLUMN task_id INTEGER REFERENCES tasks(id);
            INSERT OR IGNORE INTO contexts(name, sort_order) VALUES
                ('Job', 0), ('Personal', 1), ('Side', 2);
            CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);
            PRAGMA user_version = 2;
            COMMIT;
            "#,
        )
        .map_err(|err| format!("migration 2 failed: {err}"))?;
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

pub fn all_settings(conn: &Connection) -> Result<Vec<(String, String)>, String> {
    let mut stmt = conn
        .prepare("SELECT key, value FROM settings ORDER BY key")
        .map_err(|err| format!("could not read settings: {err}"))?;
    let rows = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|err| format!("could not read settings: {err}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("could not read settings: {err}"))
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
