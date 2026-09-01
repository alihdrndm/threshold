//! Local SQLite store. There is no server and never will be — the whole point
//! is that nothing about how you spend your attention leaves the machine.
//!
//! Accessed from Rust rather than through a JS-facing SQL plugin, because
//! streak counting, lock expiry and debounce all need the database at moments
//! when no webview is alive.

pub mod intentions;
pub mod quotes;
pub mod sessions;
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

    if version < 3 {
        // A session is what a ritual commits you to: one task, for a while.
        // Separate from `intentions` because an intention is an append-only
        // record of what you said at the start, while a session has a life -
        // it runs, it ends, and someone answers for it. Overwriting the
        // intention to hold that would destroy the before/after pair that makes
        // recording a prediction worth anything.
        //
        // Timestamps are unix seconds here rather than the RFC3339 the older
        // tables use: these are compared against the lock's `locked_until` and
        // subtracted on every poll of the expiry watcher.
        conn.execute_batch(
            r#"
            BEGIN;
            CREATE TABLE IF NOT EXISTS sessions(
                id INTEGER PRIMARY KEY,
                intention_id INTEGER NOT NULL REFERENCES intentions(id),
                task_id INTEGER REFERENCES tasks(id),
                -- Snapshot, not a join: the banner and the check-in need the
                -- title, and renaming or deleting the task later must not
                -- rewrite what a past session was about.
                task_title TEXT,
                started_ts INTEGER NOT NULL,
                ends_ts INTEGER NOT NULL,
                duration_min INTEGER NOT NULL,
                -- 1 only when the hosts file was read back and carried the block.
                enforced INTEGER NOT NULL DEFAULT 0,
                categories TEXT,
                -- Snapshot too, so scoring the check-in needs no join.
                predicted_yes INTEGER,
                state TEXT NOT NULL DEFAULT 'running' CHECK(state IN (
                    'running',
                    'awaiting_checkin',
                    'completed',
                    'partly',
                    'missed',
                    'ended_early',
                    'lapsed',
                    'unanswered'
                )),
                ended_ts INTEGER,
                answered_ts INTEGER,
                task_done INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_sessions_state ON sessions(state);
            CREATE INDEX IF NOT EXISTS idx_sessions_started ON sessions(started_ts);
            -- At most one session may be running. Two is a bug, and an insert
            -- that fails loudly beats two banners and two check-ins.
            CREATE UNIQUE INDEX IF NOT EXISTS idx_sessions_one_running
                ON sessions(state) WHERE state = 'running';
            PRAGMA user_version = 3;
            COMMIT;
            "#,
        )
        .map_err(|err| format!("migration 3 failed: {err}"))?;
    }

    if version < 4 {
        // Words the user chose, to be read at the two moments that matter: the
        // ritual, and the wall they hit when a blocked site refuses.
        //
        // The app's own copy is deliberately flat and unmotivating - praise at
        // the moment of stating an intention licenses the behaviour being
        // avoided. That rule is about *the app* congratulating you. A line you
        // picked yourself is not the app talking, and it is doing a different
        // job: at the moment of an urge, something you already believe is worth
        // more than anything this program could think to say.
        conn.execute_batch(
            r#"
            BEGIN;
            CREATE TABLE IF NOT EXISTS quotes(
                id INTEGER PRIMARY KEY,
                text TEXT NOT NULL,
                -- Optional. Plenty of the lines people keep are their own.
                author TEXT,
                created_ts TEXT NOT NULL
            );
            -- The same words twice is a mistake, not a preference.
            CREATE UNIQUE INDEX IF NOT EXISTS idx_quotes_text ON quotes(text);
            PRAGMA user_version = 4;
            COMMIT;
            "#,
        )
        .map_err(|err| format!("migration 4 failed: {err}"))?;
    }

    if version < 5 {
        // The Schedule quadrant's own meaning, written down.
        //
        // The list still has no due dates - a due date is a deadline, and
        // deadlines are exactly the teeth this feature refuses to grow. This
        // is different: "Schedule" already means "for what deserves a date",
        // and a quadrant that promised a date and never gave one was a
        // promise. `scheduled_ts` is only ever set while a task sits in
        // Schedule and is cleared the moment it leaves. It is not a deadline;
        // it is where the task lives on the calendar for as long as it lives
        // in that quadrant. The event id and link are the calendar's half of
        // the same fact.
        //
        // Unix seconds, like sessions: a slot is compared and moved, not read
        // back as text.
        conn.execute_batch(
            r#"
            BEGIN;
            ALTER TABLE tasks ADD COLUMN scheduled_ts INTEGER;
            ALTER TABLE tasks ADD COLUMN calendar_event_id TEXT;
            ALTER TABLE tasks ADD COLUMN calendar_html_link TEXT;
            PRAGMA user_version = 5;
            COMMIT;
            "#,
        )
        .map_err(|err| format!("migration 5 failed: {err}"))?;
    }

    if version < 6 {
        // A Schedule task may repeat: "1,3,5" is Mon, Wed, Fri in the same
        // Mon=1..Sun=7 vocabulary as the work_days setting; "1,2,3,4,5,6,7"
        // is daily; NULL is no repeat. The rule lives on the task, not the
        // calendar event - each occurrence is one ordinary event, and
        // completing the task advances it to the next matching day instead of
        // letting it leave the board. A weekly slot is a place, not a
        // deadline, so this stays true to the list's no-due-dates charter.
        conn.execute_batch(
            r#"
            BEGIN;
            ALTER TABLE tasks ADD COLUMN repeat_days TEXT;
            PRAGMA user_version = 6;
            COMMIT;
            "#,
        )
        .map_err(|err| format!("migration 6 failed: {err}"))?;
    }

    if version < 7 {
        // A reminder is the desktop's copy of the 10-minute popup every synced
        // event already carries on the phone. Both columns are bookkeeping for
        // that window, not new meaning on the task.
        //
        // `remind_fired_for_ts` records WHICH occurrence was already handled,
        // by storing the scheduled_ts it fired for: the reminder is spent only
        // while the two are equal, so moving the slot - reschedule, repeat
        // advance, a drag in Google - re-arms it with no clearing code at all.
        // `remind_snoozed_until` is the one deferral the user asked for from
        // the reminder itself; it never moves the slot or the calendar event.
        conn.execute_batch(
            r#"
            BEGIN;
            ALTER TABLE tasks ADD COLUMN remind_fired_for_ts INTEGER;
            ALTER TABLE tasks ADD COLUMN remind_snoozed_until INTEGER;
            PRAGMA user_version = 7;
            COMMIT;
            "#,
        )
        .map_err(|err| format!("migration 7 failed: {err}"))?;
    }

    if version < 8 {
        // The board learns to travel. `uid` is the cross-device identity -
        // rowids collide between machines, UUIDs do not - and `updated_ts` is
        // the last-writer-wins clock the Firestore channel compares.
        // `board_dirty` marks rows the next sync pass must push; every row
        // starts dirty so the first pass ships the whole board.
        //
        // The triggers are the stamping mechanism: any REAL change to a field
        // that travels bumps the clock and marks the row, no matter which
        // function wrote it. Two guards keep them honest. A write that sets
        // `updated_ts` itself (applying a remote doc) is left alone - stamping
        // it would claim the remote edit as ours and echo it back forever.
        // And per-device bookkeeping (the calendar link, reminder columns) is
        // deliberately absent from the change test - learning an event id
        // must not push a doc.
        //
        // The uid recipe is a well-formed UUIDv4 from SQLite's own randomblob,
        // so the backfill and the insert trigger need no application code.
        conn.execute_batch(
            r#"
            BEGIN;
            ALTER TABLE tasks ADD COLUMN uid TEXT;
            ALTER TABLE tasks ADD COLUMN updated_ts INTEGER NOT NULL DEFAULT 0;
            ALTER TABLE tasks ADD COLUMN board_dirty INTEGER NOT NULL DEFAULT 1;
            UPDATE tasks SET
                uid = lower(hex(randomblob(4)) || '-' || hex(randomblob(2))
                      || '-4' || substr(hex(randomblob(2)), 2) || '-'
                      || substr('89ab', abs(random()) % 4 + 1, 1)
                      || substr(hex(randomblob(2)), 2) || '-' || hex(randomblob(6))),
                updated_ts = strftime('%s','now');
            CREATE UNIQUE INDEX IF NOT EXISTS idx_tasks_uid ON tasks(uid);
            CREATE TRIGGER IF NOT EXISTS trg_tasks_board_insert
            AFTER INSERT ON tasks
            WHEN NEW.uid IS NULL
            BEGIN
                UPDATE tasks SET
                    uid = lower(hex(randomblob(4)) || '-' || hex(randomblob(2))
                          || '-4' || substr(hex(randomblob(2)), 2) || '-'
                          || substr('89ab', abs(random()) % 4 + 1, 1)
                          || substr(hex(randomblob(2)), 2) || '-' || hex(randomblob(6))),
                    updated_ts = strftime('%s','now'),
                    board_dirty = 1
                WHERE id = NEW.id;
            END;
            CREATE TRIGGER IF NOT EXISTS trg_tasks_board_update
            AFTER UPDATE ON tasks
            WHEN NEW.updated_ts = OLD.updated_ts AND (
                NEW.title IS NOT OLD.title OR
                NEW.note IS NOT OLD.note OR
                NEW.context_id IS NOT OLD.context_id OR
                NEW.urgent IS NOT OLD.urgent OR
                NEW.important IS NOT OLD.important OR
                NEW.sort_order IS NOT OLD.sort_order OR
                NEW.status IS NOT OLD.status OR
                NEW.completed_ts IS NOT OLD.completed_ts OR
                NEW.scheduled_ts IS NOT OLD.scheduled_ts OR
                NEW.repeat_days IS NOT OLD.repeat_days
            )
            BEGIN
                UPDATE tasks SET
                    updated_ts = strftime('%s','now'),
                    board_dirty = 1
                WHERE id = NEW.id;
            END;
            CREATE TRIGGER IF NOT EXISTS trg_contexts_board_insert
            AFTER INSERT ON contexts
            BEGIN
                INSERT INTO settings(key, value) VALUES('board_areas_dirty','1')
                ON CONFLICT(key) DO UPDATE SET value = '1';
            END;
            CREATE TRIGGER IF NOT EXISTS trg_contexts_board_update
            AFTER UPDATE ON contexts
            BEGIN
                INSERT INTO settings(key, value) VALUES('board_areas_dirty','1')
                ON CONFLICT(key) DO UPDATE SET value = '1';
            END;
            CREATE TRIGGER IF NOT EXISTS trg_contexts_rename_tasks
            AFTER UPDATE ON contexts
            WHEN NEW.name IS NOT OLD.name
            BEGIN
                UPDATE tasks SET
                    updated_ts = strftime('%s','now'),
                    board_dirty = 1
                WHERE context_id = NEW.id;
            END;
            CREATE TRIGGER IF NOT EXISTS trg_contexts_board_delete
            AFTER DELETE ON contexts
            BEGIN
                INSERT INTO settings(key, value) VALUES('board_areas_dirty','1')
                ON CONFLICT(key) DO UPDATE SET value = '1';
            END;
            PRAGMA user_version = 8;
            COMMIT;
            "#,
        )
        .map_err(|err| format!("migration 8 failed: {err}"))?;
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

/// The record, erased: sessions first because they point at intentions, then
/// the intentions themselves. Tasks, quotes and settings are untouched - this
/// clears the mirror, not the desk.
///
/// Deciding whether now is a safe moment (no session mid-flight) belongs to
/// the caller; this function only knows how to forget, not when.
pub fn clear_history(conn: &Connection) -> Result<(), String> {
    conn.execute("DELETE FROM sessions", [])
        .map_err(|err| format!("could not clear sessions: {err}"))?;
    conn.execute("DELETE FROM intentions", [])
        .map_err(|err| format!("could not clear intentions: {err}"))?;
    Ok(())
}

#[cfg(test)]
mod migration_tests {
    use super::*;

    /// The exact shape of the tasks table at schema version 4, before the
    /// calendar columns existed. Built by hand so the test proves the real
    /// upgrade path, not a shortcut to head.
    fn v4_database() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory database");
        conn.execute_batch(
            r#"
            BEGIN;
            CREATE TABLE settings(key TEXT PRIMARY KEY, value TEXT);
            CREATE TABLE contexts(id INTEGER PRIMARY KEY, name TEXT UNIQUE, sort_order INTEGER);
            CREATE TABLE tasks(
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
            PRAGMA user_version = 4;
            COMMIT;
            "#,
        )
        .expect("v4 schema");
        conn
    }

    #[test]
    fn upgrading_from_v4_keeps_existing_tasks_and_adds_the_calendar_columns() {
        let conn = v4_database();
        conn.execute(
            "INSERT INTO tasks(id, title, urgent, important, sort_order, status, created_ts)
             VALUES(7, 'a task that predates the calendar', 0, 1, 3, 'open', 'then')",
            [],
        )
        .unwrap();

        // The real, stepwise migration - the same one the app runs on launch.
        migrate(&conn).expect("v4 -> head migration");

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 8, "the schema advanced to head");

        // The task is untouched, and the new columns exist and default to NULL.
        let task = tasks::by_id(&conn, 7).unwrap().unwrap();
        assert_eq!(task.title, "a task that predates the calendar");
        assert_eq!(task.sort_order, 3);
        assert_eq!(task.urgent, Some(false));
        assert_eq!(task.important, Some(true));
        assert_eq!(task.scheduled_ts, None);
        assert_eq!(task.calendar_event_id, None);
        assert_eq!(task.calendar_html_link, None);
        assert_eq!(task.repeat_days, None);
    }

    /// The schema at version 5: the calendar columns exist, the repeat does
    /// not. Hand-built for the same reason as `v4_database`.
    fn v5_database() -> Connection {
        let conn = v4_database();
        conn.execute_batch(
            r#"
            BEGIN;
            ALTER TABLE tasks ADD COLUMN scheduled_ts INTEGER;
            ALTER TABLE tasks ADD COLUMN calendar_event_id TEXT;
            ALTER TABLE tasks ADD COLUMN calendar_html_link TEXT;
            PRAGMA user_version = 5;
            COMMIT;
            "#,
        )
        .expect("v5 schema");
        conn
    }

    #[test]
    fn upgrading_from_v5_keeps_the_calendar_columns_and_adds_repeat() {
        let conn = v5_database();
        conn.execute(
            "INSERT INTO tasks(id, title, urgent, important, status, created_ts,
                               scheduled_ts, calendar_event_id, calendar_html_link)
             VALUES(3, 'a scheduled task', 0, 1, 'open', 'then',
                    1700000000, 'evt', 'https://calendar.google.com/x')",
            [],
        )
        .unwrap();

        migrate(&conn).expect("v5 -> head migration");

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 8);

        let task = tasks::by_id(&conn, 3).unwrap().unwrap();
        assert_eq!(task.scheduled_ts, Some(1_700_000_000));
        assert_eq!(task.calendar_event_id.as_deref(), Some("evt"));
        assert_eq!(
            task.calendar_html_link.as_deref(),
            Some("https://calendar.google.com/x")
        );
        assert_eq!(task.repeat_days, None);
    }

    /// The schema at version 6: repeat exists, the reminder bookkeeping does
    /// not. Hand-built for the same reason as `v4_database`.
    fn v6_database() -> Connection {
        let conn = v5_database();
        conn.execute_batch(
            r#"
            BEGIN;
            ALTER TABLE tasks ADD COLUMN repeat_days TEXT;
            PRAGMA user_version = 6;
            COMMIT;
            "#,
        )
        .expect("v6 schema");
        conn
    }

    #[test]
    fn upgrading_from_v6_adds_the_reminder_columns_as_null() {
        let conn = v6_database();
        conn.execute(
            "INSERT INTO tasks(id, title, urgent, important, status, created_ts,
                               scheduled_ts, repeat_days)
             VALUES(9, 'a repeating slot', 0, 1, 'open', 'then', 1700000000, '1,3,5')",
            [],
        )
        .unwrap();

        migrate(&conn).expect("v6 -> head migration");

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 8);

        // Existing data untouched; the new columns exist and read as NULL.
        let (fired, snoozed): (Option<i64>, Option<i64>) = conn
            .query_row(
                "SELECT remind_fired_for_ts, remind_snoozed_until FROM tasks WHERE id = 9",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(fired, None);
        assert_eq!(snoozed, None);
        let task = tasks::by_id(&conn, 9).unwrap().unwrap();
        assert_eq!(task.scheduled_ts, Some(1_700_000_000));
        assert_eq!(task.repeat_days.as_deref(), Some("1,3,5"));
    }

    #[test]
    fn the_board_triggers_stamp_real_changes_and_leave_remote_applies_alone() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let id = tasks::add(
            &conn,
            &tasks::NewTask {
                title: "born local".into(),
                note: None,
                context_id: None,
            },
        )
        .unwrap();

        // A fresh local task is minted a uid and marked for the first push.
        let (uid, dirty): (Option<String>, i64) = conn
            .query_row(
                "SELECT uid, board_dirty FROM tasks WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert!(uid.is_some(), "the insert trigger minted a uid");
        assert_eq!(dirty, 1);

        // Settle the row, then make a real change: it re-stamps.
        conn.execute(
            "UPDATE tasks SET board_dirty = 0, updated_ts = 5 WHERE id = ?1",
            [id],
        )
        .unwrap();
        conn.execute("UPDATE tasks SET title = 'edited' WHERE id = ?1", [id])
            .unwrap();
        let (ts, dirty): (i64, i64) = conn
            .query_row(
                "SELECT updated_ts, board_dirty FROM tasks WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert!(ts > 5, "a real edit bumps the clock");
        assert_eq!(dirty, 1, "and marks the row for push");

        // Per-device bookkeeping (the calendar link) is not board data.
        conn.execute(
            "UPDATE tasks SET board_dirty = 0, updated_ts = 7 WHERE id = ?1",
            [id],
        )
        .unwrap();
        conn.execute(
            "UPDATE tasks SET calendar_event_id = 'evt' WHERE id = ?1",
            [id],
        )
        .unwrap();
        let (ts, dirty): (i64, i64) = conn
            .query_row(
                "SELECT updated_ts, board_dirty FROM tasks WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((ts, dirty), (7, 0), "learning a link stamps nothing");

        // A remote apply writes its own clock - the trigger must not claim
        // the edit as ours and echo it back.
        conn.execute(
            "UPDATE tasks SET title = 'from the phone', updated_ts = 99, board_dirty = 0
             WHERE id = ?1",
            [id],
        )
        .unwrap();
        let (ts, dirty): (i64, i64) = conn
            .query_row(
                "SELECT updated_ts, board_dirty FROM tasks WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((ts, dirty), (99, 0), "a remote apply stays remote");

        // A remote insert brings its uid; the mint trigger steps aside.
        conn.execute(
            "INSERT INTO tasks(title, status, created_ts, uid, updated_ts, board_dirty)
             VALUES('born elsewhere', 'open', 'now', 'mobile-uid', 50, 0)",
            [],
        )
        .unwrap();
        let (uid, ts, dirty): (String, i64, i64) = conn
            .query_row(
                "SELECT uid, updated_ts, board_dirty FROM tasks WHERE uid = 'mobile-uid'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!((uid.as_str(), ts, dirty), ("mobile-uid", 50, 0));

        // Touching an area marks the areas list for push.
        conn.execute("DELETE FROM settings WHERE key = 'board_areas_dirty'", [])
            .unwrap();
        tasks::add_context(&conn, "Errands").unwrap();
        assert_eq!(
            get_setting(&conn, "board_areas_dirty").as_deref(),
            Some("1")
        );
    }

    #[test]
    fn migrating_an_already_current_database_is_a_no_op() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let before: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        // Running it again must not fail on a duplicate ADD COLUMN.
        migrate(&conn).unwrap();
        let after: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(before, after);
        assert_eq!(after, 8);
    }
}
