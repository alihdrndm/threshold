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
        // New tasks join the end of the Inbox rather than all landing at 0.
        // Before zones could be reordered every task carried 0 and creation
        // order fell out of the id tie-break; once a reorder renumbers a zone
        // 0..n, a fresh 0 would cut into the middle of an arranged list.
        "INSERT INTO tasks(title, note, context_id, urgent, important, sort_order, status, created_ts)
         VALUES(?1, ?2, ?3, NULL, NULL,
                (SELECT COALESCE(MAX(sort_order) + 1, 0) FROM tasks
                  WHERE status = 'open' AND urgent IS NULL AND important IS NULL),
                'open', ?4)",
        rusqlite::params![title, task.note, task.context_id, chrono::Utc::now().to_rfc3339()],
    )
    .map_err(|err| format!("could not add task: {err}"))?;
    Ok(conn.last_insert_rowid())
}

/// Persist a zone's order as the user arranged it: each id gets its index.
///
/// Scoped to exactly the ids given - tasks outside the list keep their numbers,
/// so renumbering one quadrant cannot scramble another, and the `(sort_order,
/// id)` tie-break keeps legacy all-zero zones in the order they always showed.
pub fn reorder(conn: &Connection, ids: &[i64]) -> Result<(), String> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|err| format!("could not reorder tasks: {err}"))?;
    {
        let mut stmt = tx
            .prepare("UPDATE tasks SET sort_order = ?1 WHERE id = ?2")
            .map_err(|err| format!("could not reorder tasks: {err}"))?;
        for (index, id) in ids.iter().enumerate() {
            stmt.execute(rusqlite::params![index as i64, id])
                .map_err(|err| format!("could not reorder tasks: {err}"))?;
        }
    }
    tx.commit()
        .map_err(|err| format!("could not reorder tasks: {err}"))
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

/// Move a task to a quadrant, joining the end of whatever is already there.
///
/// `set_quadrant` takes the caller's word for a position; this one asks the
/// zone. It exists for callers who are not looking at the board - the check-in
/// scheduling a task cannot know where the Schedule quadrant currently ends.
/// `IS`, not `=`: both flags may be NULL, and `= NULL` matches nothing.
pub fn append_to_quadrant(
    conn: &Connection,
    id: i64,
    urgent: Option<bool>,
    important: Option<bool>,
) -> Result<(), String> {
    let changed = conn
        .execute(
            "UPDATE tasks SET urgent = ?1, important = ?2,
                    sort_order = (SELECT COALESCE(MAX(sort_order) + 1, 0) FROM tasks
                                   WHERE status = 'open' AND urgent IS ?1 AND important IS ?2
                                     AND id != ?3)
             WHERE id = ?3",
            rusqlite::params![
                urgent.map(|v| v as i64),
                important.map(|v| v as i64),
                id
            ],
        )
        .map_err(|err| format!("could not move task: {err}"))?;
    if changed == 0 {
        return Err(format!("no task with id {id}"));
    }
    Ok(())
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
    fn appending_joins_the_end_of_the_quadrant() {
        let conn = memory_db();
        let settled = add(&conn, &new("already scheduled")).unwrap();
        set_quadrant(&conn, settled, Some(false), Some(true), 5).unwrap();
        let arriving = add(&conn, &new("scheduled from the check-in")).unwrap();
        append_to_quadrant(&conn, arriving, Some(false), Some(true)).unwrap();

        let tasks = open_tasks(&conn).unwrap();
        let task = tasks.iter().find(|t| t.id == arriving).unwrap();
        assert_eq!(task.urgent, Some(false));
        assert_eq!(task.important, Some(true));
        assert_eq!(task.sort_order, 6, "after the settled task, not among it");
        assert!(append_to_quadrant(&conn, 999, Some(false), Some(true)).is_err());
    }

    #[test]
    fn new_tasks_join_the_inbox_at_the_end() {
        let conn = memory_db();
        add(&conn, &new("first")).unwrap();
        add(&conn, &new("second")).unwrap();
        let orders: Vec<i64> = open_tasks(&conn).unwrap().iter().map(|t| t.sort_order).collect();
        assert_eq!(orders, vec![0, 1]);
    }

    #[test]
    fn reorder_rewrites_the_listed_order() {
        let conn = memory_db();
        let a = add(&conn, &new("a")).unwrap();
        let b = add(&conn, &new("b")).unwrap();
        let c = add(&conn, &new("c")).unwrap();
        reorder(&conn, &[c, a, b]).unwrap();
        let titles: Vec<String> =
            open_tasks(&conn).unwrap().into_iter().map(|t| t.title).collect();
        assert_eq!(titles, vec!["c", "a", "b"]);
    }

    #[test]
    fn reorder_leaves_unlisted_tasks_alone() {
        let conn = memory_db();
        let a = add(&conn, &new("a")).unwrap();
        let b = add(&conn, &new("b")).unwrap();
        let c = add(&conn, &new("c")).unwrap();
        // Renumber a and c only - b keeps the 1 it was born with, the way a
        // context filter reordering the visible subset must not move the rest.
        reorder(&conn, &[c, a]).unwrap();
        let tasks = open_tasks(&conn).unwrap();
        let of = |id: i64| tasks.iter().find(|t| t.id == id).unwrap().sort_order;
        assert_eq!((of(c), of(a), of(b)), (0, 1, 1));
    }

    #[test]
    fn the_ritual_chips_follow_the_arranged_order() {
        let conn = memory_db();
        let a = add(&conn, &new("second priority")).unwrap();
        let b = add(&conn, &new("first priority")).unwrap();
        set_quadrant(&conn, a, Some(true), Some(true), 0).unwrap();
        set_quadrant(&conn, b, Some(true), Some(true), 0).unwrap();
        reorder(&conn, &[b, a]).unwrap();
        let titles: Vec<String> =
            do_first(&conn, 3).unwrap().into_iter().map(|(_, t)| t).collect();
        assert_eq!(
            titles,
            vec!["first priority".to_string(), "second priority".to_string()],
            "dragging a task to the top of Do First must also promote its chip"
        );
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
