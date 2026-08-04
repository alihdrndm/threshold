//! The refined to-do list (spec F6).
//!
//! A short list, not a project manager. There are no due dates, no recurrence,
//! no subtasks and no tags on purpose: the interceptor is the product, and a
//! task feature that grows teeth starts competing with it for attention.
//!
//! Position on the Eisenhower matrix is stored as two flags rather than a
//! quadrant name, so "unclassified" has an honest representation - both NULL,
//! which is the Inbox.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

// The "Do First" soft cap lives in the UI (windows/dashboard/tasks/quadrants.ts)
// rather than here. It is a nudge shown inline, never a rule the data layer
// enforces - and a constant defined in two places is a constant that drifts.

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Context {
    pub id: i64,
    pub name: String,
    pub sort_order: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: i64,
    pub title: String,
    pub note: Option<String>,
    pub context_id: Option<i64>,
    /// Both None means the Inbox: not yet classified.
    pub urgent: Option<bool>,
    pub important: Option<bool>,
    pub sort_order: i64,
    pub status: String,
    pub created_ts: String,
    pub completed_ts: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewTask {
    pub title: String,
    pub note: Option<String>,
    pub context_id: Option<i64>,
}

fn row_to_task(row: &rusqlite::Row<'_>) -> rusqlite::Result<Task> {
    Ok(Task {
        id: row.get(0)?,
        title: row.get(1)?,
        note: row.get(2)?,
        context_id: row.get(3)?,
        urgent: row.get::<_, Option<i64>>(4)?.map(|v| v != 0),
        important: row.get::<_, Option<i64>>(5)?.map(|v| v != 0),
        sort_order: row.get(6)?,
        status: row.get(7)?,
        created_ts: row.get(8)?,
        completed_ts: row.get(9)?,
    })
}

const SELECT: &str = "SELECT id, title, note, context_id, urgent, important, sort_order, status,
                             created_ts, completed_ts FROM tasks";

pub fn contexts(conn: &Connection) -> Result<Vec<Context>, String> {
    let mut stmt = conn
        .prepare("SELECT id, name, sort_order FROM contexts ORDER BY sort_order, id")
        .map_err(|err| format!("could not read contexts: {err}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok(Context {
                id: row.get(0)?,
                name: row.get(1)?,
                sort_order: row.get(2)?,
            })
        })
        .map_err(|err| format!("could not read contexts: {err}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("could not read contexts: {err}"))
}

/// Everything still on the plate. Done and archived tasks stay in the database
/// but out of the way.
pub fn open_tasks(conn: &Connection) -> Result<Vec<Task>, String> {
    let mut stmt = conn
        .prepare(&format!(
            "{SELECT} WHERE status IN ('open','done') ORDER BY sort_order, id"
        ))
        .map_err(|err| format!("could not read tasks: {err}"))?;
    let rows = stmt
        .query_map([], row_to_task)
        .map_err(|err| format!("could not read tasks: {err}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("could not read tasks: {err}"))
}

pub fn add(conn: &Connection, task: &NewTask) -> Result<i64, String> {
    let title = task.title.trim();
    if title.is_empty() {
        return Err("a task needs a title".into());
    }
    conn.execute(
        "INSERT INTO tasks(title, note, context_id, urgent, important, sort_order, status, created_ts)
         VALUES(?1, ?2, ?3, NULL, NULL, 0, 'open', ?4)",
        rusqlite::params![title, task.note, task.context_id, chrono::Utc::now().to_rfc3339()],
    )
    .map_err(|err| format!("could not add task: {err}"))?;
    Ok(conn.last_insert_rowid())
}

/// Move a task to a quadrant. `None`/`None` returns it to the Inbox.
pub fn set_quadrant(
    conn: &Connection,
    id: i64,
    urgent: Option<bool>,
    important: Option<bool>,
    sort_order: i64,
) -> Result<(), String> {
    conn.execute(
        "UPDATE tasks SET urgent = ?1, important = ?2, sort_order = ?3 WHERE id = ?4",
        rusqlite::params![
            urgent.map(|v| v as i64),
            important.map(|v| v as i64),
            sort_order,
            id
        ],
    )
    .map(|_| ())
    .map_err(|err| format!("could not move task: {err}"))
}

pub fn set_status(conn: &Connection, id: i64, status: &str) -> Result<(), String> {
    if !matches!(status, "open" | "done" | "archived" | "deleted") {
        return Err(format!("unknown status: {status}"));
    }
    let completed = if status == "done" {
        Some(chrono::Utc::now().to_rfc3339())
    } else {
        None
    };
    conn.execute(
        "UPDATE tasks SET status = ?1, completed_ts = ?2 WHERE id = ?3",
        rusqlite::params![status, completed, id],
    )
    .map(|_| ())
    .map_err(|err| format!("could not update task: {err}"))
}

pub fn title_of(conn: &Connection, id: i64) -> Result<String, String> {
    conn.query_row("SELECT title FROM tasks WHERE id = ?1", [id], |row| {
        row.get::<_, String>(0)
    })
    .map_err(|_| format!("no task with id {id}"))
}

/// Is this task still open?
///
/// Asked before offering to tick one off from the check-in: `set_status` on a
/// deleted task would set it back to 'done', and `open_tasks` includes 'done',
/// so a forgotten question could resurrect a deleted task into the matrix.
pub fn is_open(conn: &Connection, id: i64) -> Result<bool, String> {
    conn.query_row(
        "SELECT status FROM tasks WHERE id = ?1",
        [id],
        |row| row.get::<_, String>(0),
    )
    .map(|status| status == "open")
    .map_err(|err| format!("could not read task {id}: {err}"))
}

/// The "Do First" quadrant, for the ritual's one-click chips.
///
/// This is the point of the whole feature: at the vulnerable moment you are
/// shown what matters rather than asked to remember it.
///
/// Carries the id alongside the title so that picking a chip links the record
/// to its task. Returning titles alone was why only the Focus button could ever
/// populate `intentions.task_id`.
pub fn do_first(conn: &Connection, limit: i64) -> Result<Vec<(i64, String)>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, title FROM tasks
             WHERE status = 'open' AND urgent = 1 AND important = 1
             ORDER BY sort_order, id LIMIT ?1",
        )
        .map_err(|err| format!("could not read do-first tasks: {err}"))?;
    let rows = stmt
        .query_map([limit], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|err| format!("could not read do-first tasks: {err}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("could not read do-first tasks: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory_db() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory database");
        crate::db::migrate_for_tests(&conn);
        conn
    }

    fn new(title: &str) -> NewTask {
        NewTask {
            title: title.into(),
            note: None,
            context_id: None,
        }
    }

    #[test]
    fn the_seed_contexts_exist() {
        let conn = memory_db();
        let names: Vec<String> = contexts(&conn).unwrap().into_iter().map(|c| c.name).collect();
        assert!(names.contains(&"Job".to_string()));
        assert!(names.contains(&"Personal".to_string()));
        assert!(names.contains(&"Side".to_string()));
    }

    #[test]
    fn a_new_task_lands_in_the_inbox_unclassified() {
        let conn = memory_db();
        add(&conn, &new("write the report")).unwrap();
        let task = &open_tasks(&conn).unwrap()[0];
        assert_eq!(task.urgent, None);
        assert_eq!(task.important, None);
        assert_eq!(task.status, "open");
    }

    #[test]
    fn a_blank_title_is_refused() {
        let conn = memory_db();
        assert!(add(&conn, &new("   ")).is_err());
    }

    #[test]
    fn moving_to_a_quadrant_sets_both_flags() {
        let conn = memory_db();
        let id = add(&conn, &new("write the report")).unwrap();
        set_quadrant(&conn, id, Some(true), Some(true), 0).unwrap();
        let task = &open_tasks(&conn).unwrap()[0];
        assert_eq!(task.urgent, Some(true));
        assert_eq!(task.important, Some(true));
    }

    #[test]
    fn a_task_can_return_to_the_inbox() {
        let conn = memory_db();
        let id = add(&conn, &new("write the report")).unwrap();
        set_quadrant(&conn, id, Some(false), Some(false), 0).unwrap();
        set_quadrant(&conn, id, None, None, 0).unwrap();
        let task = &open_tasks(&conn).unwrap()[0];
        assert_eq!(task.urgent, None);
    }

    #[test]
    fn only_do_first_tasks_reach_the_ritual() {
        let conn = memory_db();
        let a = add(&conn, &new("urgent and important")).unwrap();
        let b = add(&conn, &new("merely important")).unwrap();
        set_quadrant(&conn, a, Some(true), Some(true), 0).unwrap();
        set_quadrant(&conn, b, Some(false), Some(true), 0).unwrap();
        let chips = do_first(&conn, 3).unwrap();
        assert_eq!(
            chips,
            vec![(a, "urgent and important".to_string())],
            "the chip must carry its task id, or picking it cannot link the record"
        );
    }

    #[test]
    fn a_completed_task_stops_being_offered() {
        let conn = memory_db();
        let id = add(&conn, &new("done already")).unwrap();
        set_quadrant(&conn, id, Some(true), Some(true), 0).unwrap();
        set_status(&conn, id, "done").unwrap();
        assert!(do_first(&conn, 3).unwrap().is_empty());
    }

    #[test]
    fn completing_a_task_records_when() {
        let conn = memory_db();
        let id = add(&conn, &new("finish")).unwrap();
        set_status(&conn, id, "done").unwrap();
        let task = &open_tasks(&conn).unwrap()[0];
        assert_eq!(task.status, "done");
        assert!(task.completed_ts.is_some());
    }

    #[test]
    fn an_unknown_status_is_refused() {
        let conn = memory_db();
        let id = add(&conn, &new("finish")).unwrap();
        assert!(set_status(&conn, id, "procrastinating").is_err());
    }

    #[test]
    fn deleted_tasks_leave_the_list_but_stay_in_the_database() {
        let conn = memory_db();
        let id = add(&conn, &new("gone")).unwrap();
        set_status(&conn, id, "deleted").unwrap();
        assert!(open_tasks(&conn).unwrap().is_empty());
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }
}
