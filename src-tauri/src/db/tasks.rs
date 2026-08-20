//! The refined to-do list (spec F6).
//!
//! A short list, not a project manager. There are no due dates, no subtasks
//! and no tags on purpose: the interceptor is the product, and a task feature
//! that grows teeth starts competing with it for attention. The one recurrence
//! the list allows is Schedule's own (`repeat_days`, below): a weekly slot is
//! a place a task returns to, not a deadline that chases it.
//!
//! Position on the Eisenhower matrix is stored as two flags rather than a
//! quadrant name, so "unclassified" has an honest representation - both NULL,
//! which is the Inbox.
//!
//! One exception to "no dates", and it is the quadrant's own: a task in
//! Schedule carries the slot it holds on the calendar (`scheduled_ts`, unix
//! seconds) and the event behind it. Not a deadline - a place. Set only while
//! the task sits in Schedule, cleared when it leaves.

use rusqlite::{Connection, OptionalExtension};
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
    /// The slot this task holds on the calendar, while it sits in Schedule.
    pub scheduled_ts: Option<i64>,
    pub calendar_event_id: Option<String>,
    pub calendar_html_link: Option<String>,
    /// Days this Schedule task repeats on: "1,3,5" = Mon, Wed, Fri (Mon=1..
    /// Sun=7, the work_days vocabulary). NULL = no repeat. Cleared when the
    /// task leaves the quadrant - the rule is Schedule's, not the task's.
    pub repeat_days: Option<String>,
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
        scheduled_ts: row.get(10)?,
        calendar_event_id: row.get(11)?,
        calendar_html_link: row.get(12)?,
        repeat_days: row.get(13)?,
    })
}

const SELECT: &str = "SELECT id, title, note, context_id, urgent, important, sort_order, status,
                             created_ts, completed_ts, scheduled_ts, calendar_event_id,
                             calendar_html_link, repeat_days FROM tasks";

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

/// The most areas a person can hold in their head as *areas*. Past this they
/// are tags, and tags are the feature this list refuses to grow (see the module
/// note). A nudge enforced here so no caller can quietly bypass it.
pub const MAX_CONTEXTS: usize = 8;

fn tidy_context_name(name: &str) -> Result<String, String> {
    // The '#' is quick-add syntax, not part of the name.
    let name = name
        .trim()
        .trim_start_matches('#')
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if name.is_empty() {
        return Err("an area needs a name".into());
    }
    if name.chars().count() > 24 {
        return Err("an area's name is at most 24 characters".into());
    }
    Ok(name)
}

/// Add an area, joining the end of the row. Names are unique, case-insensitively:
/// two areas that differ only in case would be one area with a typo.
pub fn add_context(conn: &Connection, name: &str) -> Result<i64, String> {
    let name = tidy_context_name(name)?;
    let existing = contexts(conn)?;
    if existing.len() >= MAX_CONTEXTS {
        return Err(format!(
            "{MAX_CONTEXTS} areas is the most this list will hold - past that they stop being areas"
        ));
    }
    if let Some(taken) = existing.iter().find(|c| c.name.eq_ignore_ascii_case(&name)) {
        return Err(format!("there is already an area called {}", taken.name));
    }
    conn.execute(
        "INSERT INTO contexts(name, sort_order)
         VALUES(?1, (SELECT COALESCE(MAX(sort_order) + 1, 0) FROM contexts))",
        [&name],
    )
    .map_err(|err| format!("could not add the area: {err}"))?;
    Ok(conn.last_insert_rowid())
}

pub fn rename_context(conn: &Connection, id: i64, name: &str) -> Result<(), String> {
    let name = tidy_context_name(name)?;
    if let Some(taken) = contexts(conn)?
        .iter()
        .find(|c| c.id != id && c.name.eq_ignore_ascii_case(&name))
    {
        return Err(format!("there is already an area called {}", taken.name));
    }
    let changed = conn
        .execute(
            "UPDATE contexts SET name = ?1 WHERE id = ?2",
            rusqlite::params![name, id],
        )
        .map_err(|err| format!("could not rename the area: {err}"))?;
    if changed == 0 {
        return Err(format!("no area with id {id}"));
    }
    Ok(())
}

/// Remove an area. Its tasks lose the area and keep everything else - deleting
/// a label must never delete the things it labelled.
pub fn remove_context(conn: &Connection, id: i64) -> Result<(), String> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|err| format!("could not remove the area: {err}"))?;
    tx.execute("UPDATE tasks SET context_id = NULL WHERE context_id = ?1", [id])
        .map_err(|err| format!("could not remove the area: {err}"))?;
    let changed = tx
        .execute("DELETE FROM contexts WHERE id = ?1", [id])
        .map_err(|err| format!("could not remove the area: {err}"))?;
    if changed == 0 {
        return Err(format!("no area with id {id}"));
    }
    tx.commit()
        .map_err(|err| format!("could not remove the area: {err}"))
}

/// Give a task an area, or `None` to take it away.
pub fn set_context(conn: &Connection, id: i64, context_id: Option<i64>) -> Result<(), String> {
    let changed = conn
        .execute(
            "UPDATE tasks SET context_id = ?1 WHERE id = ?2",
            rusqlite::params![context_id, id],
        )
        .map_err(|err| format!("could not move the task: {err}"))?;
    if changed == 0 {
        return Err(format!("no task with id {id}"));
    }
    Ok(())
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
///
/// Leaving Schedule takes the repeat with it: the rule belongs to the
/// quadrant, and a task dragged to Do First should not quietly come back next
/// Tuesday. The CASE keeps it only when the destination IS Schedule (NULL
/// flags fall through to ELSE, so the Inbox clears too).
pub fn set_quadrant(
    conn: &Connection,
    id: i64,
    urgent: Option<bool>,
    important: Option<bool>,
    sort_order: i64,
) -> Result<(), String> {
    conn.execute(
        "UPDATE tasks SET urgent = ?1, important = ?2, sort_order = ?3,
                repeat_days = CASE WHEN ?1 = 0 AND ?2 = 1 THEN repeat_days ELSE NULL END
         WHERE id = ?4",
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
                    repeat_days = CASE WHEN ?1 = 0 AND ?2 = 1 THEN repeat_days ELSE NULL END,
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

/// Is this task in the Schedule quadrant (not urgent, important)? A task that
/// does not exist is not in Schedule - the honest answer, not an error.
pub fn is_in_schedule(conn: &Connection, id: i64) -> Result<bool, String> {
    conn.query_row(
        "SELECT urgent = 0 AND important = 1 FROM tasks WHERE id = ?1",
        [id],
        |row| row.get::<_, Option<bool>>(0),
    )
    .optional()
    .map(|v| v.flatten().unwrap_or(false))
    .map_err(|err| format!("could not read task {id}: {err}"))
}

/// One task, whatever its status.
pub fn by_id(conn: &Connection, id: i64) -> Result<Option<Task>, String> {
    let mut stmt = conn
        .prepare(&format!("{SELECT} WHERE id = ?1"))
        .map_err(|err| format!("could not read task {id}: {err}"))?;
    let mut rows = stmt
        .query_map([id], row_to_task)
        .map_err(|err| format!("could not read task {id}: {err}"))?;
    match rows.next() {
        None => Ok(None),
        Some(row) => row
            .map(Some)
            .map_err(|err| format!("could not read task {id}: {err}")),
    }
}

/// Record the slot a task holds and the event that holds it.
///
/// A `None` link KEEPS the stored one (COALESCE): `reschedule` and the poll's
/// moved-branch have no link to give - patch does not return htmlLink - and
/// must not erase the one we have. Erasing is `clear_schedule`'s job.
pub fn set_schedule(
    conn: &Connection,
    id: i64,
    scheduled_ts: Option<i64>,
    event_id: Option<&str>,
    html_link: Option<&str>,
) -> Result<(), String> {
    conn.execute(
        "UPDATE tasks SET scheduled_ts = ?1, calendar_event_id = ?2,
                calendar_html_link = COALESCE(?3, calendar_html_link)
         WHERE id = ?4",
        rusqlite::params![scheduled_ts, event_id, html_link, id],
    )
    .map(|_| ())
    .map_err(|err| format!("could not record the slot: {err}"))
}

/// Forget the slot, the event and its link. The task itself - including any
/// repeat rule - is untouched: losing an occurrence is not losing the habit.
pub fn clear_schedule(conn: &Connection, id: i64) -> Result<(), String> {
    conn.execute(
        "UPDATE tasks SET scheduled_ts = NULL, calendar_event_id = NULL,
                calendar_html_link = NULL
         WHERE id = ?1",
        [id],
    )
    .map(|_| ())
    .map_err(|err| format!("could not clear the slot: {err}"))
}

/// Set or clear how a task repeats. Validation is the command's job; this
/// records exactly what it is given.
pub fn set_repeat_days(conn: &Connection, id: i64, days: Option<&str>) -> Result<(), String> {
    let changed = conn
        .execute(
            "UPDATE tasks SET repeat_days = ?1 WHERE id = ?2",
            rusqlite::params![days, id],
        )
        .map_err(|err| format!("could not set the repeat: {err}"))?;
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
    fn areas_can_be_added_renamed_and_removed_without_losing_tasks() {
        let conn = memory_db();
        let health = add_context(&conn, "  #Health  ").unwrap();
        assert!(contexts(&conn).unwrap().iter().any(|c| c.name == "Health"));

        let id = add(&conn, &new("run")).unwrap();
        set_context(&conn, id, Some(health)).unwrap();
        assert_eq!(open_tasks(&conn).unwrap()[0].context_id, Some(health));

        rename_context(&conn, health, "Body").unwrap();
        assert!(contexts(&conn).unwrap().iter().any(|c| c.name == "Body"));

        remove_context(&conn, health).unwrap();
        let task = &open_tasks(&conn).unwrap()[0];
        assert_eq!(task.context_id, None, "the task survives, unlabelled");
        assert_eq!(task.title, "run");
    }

    #[test]
    fn area_names_are_unique_regardless_of_case_and_capped() {
        let conn = memory_db();
        assert!(add_context(&conn, "job").is_err(), "Job already exists");
        assert!(add_context(&conn, "   ").is_err());
        assert!(add_context(&conn, " # ").is_err(), "a bare hash is not a name");
        for i in 0..5 {
            add_context(&conn, &format!("area {i}")).unwrap();
        }
        assert_eq!(contexts(&conn).unwrap().len(), MAX_CONTEXTS);
        assert!(add_context(&conn, "one too many").is_err());
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
    fn a_slot_move_keeps_the_calendar_link() {
        let conn = memory_db();
        let id = add(&conn, &new("weekly review")).unwrap();
        set_schedule(&conn, id, Some(1_000), Some("evt"), Some("https://x")).unwrap();
        // A move with no link to give (patch returns none) keeps the stored one.
        set_schedule(&conn, id, Some(2_000), Some("evt"), None).unwrap();
        let task = by_id(&conn, id).unwrap().unwrap();
        assert_eq!(task.scheduled_ts, Some(2_000));
        assert_eq!(task.calendar_html_link.as_deref(), Some("https://x"));
        // Clearing forgets all three.
        clear_schedule(&conn, id).unwrap();
        let task = by_id(&conn, id).unwrap().unwrap();
        assert_eq!(task.scheduled_ts, None);
        assert_eq!(task.calendar_event_id, None);
        assert_eq!(task.calendar_html_link, None);
    }

    #[test]
    fn repeat_days_are_set_and_cleared() {
        let conn = memory_db();
        let id = add(&conn, &new("morning pages")).unwrap();
        set_repeat_days(&conn, id, Some("1,3,5")).unwrap();
        assert_eq!(
            by_id(&conn, id).unwrap().unwrap().repeat_days.as_deref(),
            Some("1,3,5")
        );
        set_repeat_days(&conn, id, None).unwrap();
        assert_eq!(by_id(&conn, id).unwrap().unwrap().repeat_days, None);
        assert!(set_repeat_days(&conn, 999, Some("1")).is_err());
    }

    #[test]
    fn leaving_schedule_clears_the_repeat() {
        let conn = memory_db();
        let id = add(&conn, &new("weekly review")).unwrap();
        set_quadrant(&conn, id, Some(false), Some(true), 0).unwrap();
        set_repeat_days(&conn, id, Some("2")).unwrap();
        // Dragged to Do First: the rule stays behind.
        set_quadrant(&conn, id, Some(true), Some(true), 0).unwrap();
        assert_eq!(by_id(&conn, id).unwrap().unwrap().repeat_days, None);
        // And via the appending mover, to the Inbox.
        set_repeat_days(&conn, id, Some("2")).unwrap();
        append_to_quadrant(&conn, id, None, None).unwrap();
        assert_eq!(by_id(&conn, id).unwrap().unwrap().repeat_days, None);
    }

    #[test]
    fn moving_within_schedule_keeps_the_repeat() {
        let conn = memory_db();
        let id = add(&conn, &new("weekly review")).unwrap();
        set_quadrant(&conn, id, Some(false), Some(true), 0).unwrap();
        set_repeat_days(&conn, id, Some("1,2,3,4,5,6,7")).unwrap();
        // A reorder inside the quadrant is still a set_quadrant call.
        set_quadrant(&conn, id, Some(false), Some(true), 4).unwrap();
        assert_eq!(
            by_id(&conn, id).unwrap().unwrap().repeat_days.as_deref(),
            Some("1,2,3,4,5,6,7")
        );
        append_to_quadrant(&conn, id, Some(false), Some(true)).unwrap();
        assert!(by_id(&conn, id).unwrap().unwrap().repeat_days.is_some());
    }

    #[test]
    fn a_deleted_task_keeps_its_repeat_for_undo() {
        let conn = memory_db();
        let id = add(&conn, &new("weekly review")).unwrap();
        set_quadrant(&conn, id, Some(false), Some(true), 0).unwrap();
        set_repeat_days(&conn, id, Some("4")).unwrap();
        set_status(&conn, id, "deleted").unwrap();
        assert_eq!(
            by_id(&conn, id).unwrap().unwrap().repeat_days.as_deref(),
            Some("4"),
            "undo restores the habit intact"
        );
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
    fn a_slot_is_recorded_and_forgotten() {
        let conn = memory_db();
        let id = add(&conn, &new("plan the week")).unwrap();
        set_quadrant(&conn, id, Some(false), Some(true), 0).unwrap();
        assert!(is_in_schedule(&conn, id).unwrap());
        assert!(!is_in_schedule(&conn, 999).unwrap());

        set_schedule(&conn, id, Some(1_700_000_000), Some("evt1"), Some("https://x")).unwrap();
        let task = by_id(&conn, id).unwrap().unwrap();
        assert_eq!(task.scheduled_ts, Some(1_700_000_000));
        assert_eq!(task.calendar_event_id.as_deref(), Some("evt1"));

        clear_schedule(&conn, id).unwrap();
        let task = by_id(&conn, id).unwrap().unwrap();
        assert_eq!(task.scheduled_ts, None);
        assert_eq!(task.calendar_event_id, None);
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
