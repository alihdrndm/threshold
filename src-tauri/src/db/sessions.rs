//! What a ritual committed you to, and how it went.
//!
//! A session is one task for a while. It is deliberately *not* the same thing
//! as a block: blocking is an optional enforcement layer that a session may or
//! may not carry, and welding the two together is why choosing no categories
//! used to produce four screens of ritual and then nothing at all.
//!
//! # This table is a record, not a lock
//!
//! It lives in the user's own database, which the user can edit. That is fine,
//! because nothing here decides whether a block may be lifted — the helper and
//! its ACL'd `lock.json` own that, on a monotonic clock. **No code may consult
//! this table when deciding to unblock.** The moment something does, the
//! commitment is defeated by an UPDATE statement.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// Where a session is in its life.
///
/// `unanswered` and `missed` are deliberately distinct. "I walked away" and "I
/// sat here and did not do it" are different facts, and collapsing them into
/// one would quietly poison the only thing the check-in exists to measure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// Counting down.
    Running,
    /// Time is up and the question is owed.
    AwaitingCheckin,
    Completed,
    Partly,
    /// Answered: no, it did not happen.
    Missed,
    /// Stopped before its time, with no answer given.
    EndedEarly,
    /// Ran out while nobody was around to be asked.
    Lapsed,
    /// Asked, and closed without answering.
    Unanswered,
}

impl State {
    pub fn as_str(self) -> &'static str {
        match self {
            State::Running => "running",
            State::AwaitingCheckin => "awaiting_checkin",
            State::Completed => "completed",
            State::Partly => "partly",
            State::Missed => "missed",
            State::EndedEarly => "ended_early",
            State::Lapsed => "lapsed",
            State::Unanswered => "unanswered",
        }
    }

    fn from_str(value: &str) -> State {
        match value {
            "running" => State::Running,
            "awaiting_checkin" => State::AwaitingCheckin,
            "completed" => State::Completed,
            "partly" => State::Partly,
            "missed" => State::Missed,
            "ended_early" => State::EndedEarly,
            "lapsed" => State::Lapsed,
            _ => State::Unanswered,
        }
    }
}

/// What the user said at the end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Answer {
    DidIt,
    Partly,
    No,
}

impl Answer {
    pub fn into_state(self) -> State {
        match self {
            Answer::DidIt => State::Completed,
            Answer::Partly => State::Partly,
            Answer::No => State::Missed,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NewSession {
    pub intention_id: i64,
    pub task_id: Option<i64>,
    pub task_title: Option<String>,
    pub duration_min: i64,
    pub categories: Option<String>,
    pub predicted_yes: Option<bool>,
}

/// A session as stored.
///
/// `intention_id`, `answered_ts` and `task_done` are written and asserted in
/// tests but not yet read by the running app - the Overview's prediction
/// calibration is what will read them. They stay here because this type is the
/// row, and a row representation that omits columns invites a second, partial
/// one alongside it.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct SessionRow {
    pub id: i64,
    pub intention_id: i64,
    pub task_id: Option<i64>,
    pub task_title: Option<String>,
    pub started_ts: i64,
    pub ends_ts: i64,
    pub duration_min: i64,
    pub enforced: bool,
    pub categories: Option<String>,
    pub predicted_yes: Option<bool>,
    pub state: State,
    pub ended_ts: Option<i64>,
    pub answered_ts: Option<i64>,
    pub task_done: bool,
}

const COLUMNS: &str = "id, intention_id, task_id, task_title, started_ts, ends_ts, duration_min, \
                       enforced, categories, predicted_yes, state, ended_ts, answered_ts, task_done";

fn read(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionRow> {
    Ok(SessionRow {
        id: row.get(0)?,
        intention_id: row.get(1)?,
        task_id: row.get(2)?,
        task_title: row.get(3)?,
        started_ts: row.get(4)?,
        ends_ts: row.get(5)?,
        duration_min: row.get(6)?,
        enforced: row.get::<_, i64>(7)? != 0,
        categories: row.get(8)?,
        predicted_yes: row.get::<_, Option<i64>>(9)?.map(|v| v != 0),
        state: State::from_str(&row.get::<_, String>(10)?),
        ended_ts: row.get(11)?,
        answered_ts: row.get(12)?,
        task_done: row.get::<_, i64>(13)? != 0,
    })
}

/// Begin a session. `now` is passed in so callers can keep one clock reading
/// across the intention and the session they write together.
pub fn start(conn: &Connection, new: &NewSession, now: i64) -> Result<i64, String> {
    conn.execute(
        "INSERT INTO sessions(intention_id, task_id, task_title, started_ts, ends_ts,
                              duration_min, categories, predicted_yes, state)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'running')",
        rusqlite::params![
            new.intention_id,
            new.task_id,
            new.task_title,
            now,
            now + new.duration_min.max(0) * 60,
            new.duration_min,
            new.categories,
            new.predicted_yes.map(|yes| yes as i64),
        ],
    )
    .map_err(|err| format!("could not start the session: {err}"))?;
    Ok(conn.last_insert_rowid())
}

/// Realign the session's end with the lock, once the helper has confirmed.
///
/// Arming takes up to twelve seconds and the helper stamps its own clock, so
/// the two ends can differ by that much. Left alone, the banner counts to zero
/// while the sites are still blocked, or the reverse.
pub fn confirm_block(conn: &Connection, id: i64, locked_until: i64) -> Result<(), String> {
    conn.execute(
        "UPDATE sessions SET enforced = 1, ends_ts = ?2 WHERE id = ?1",
        rusqlite::params![id, locked_until],
    )
    .map_err(|err| format!("could not confirm the block: {err}"))?;
    Ok(())
}

fn one(conn: &Connection, state: State) -> Result<Option<SessionRow>, String> {
    let sql = format!("SELECT {COLUMNS} FROM sessions WHERE state = ?1 ORDER BY id DESC LIMIT 1");
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|err| format!("could not read sessions: {err}"))?;
    let mut rows = stmt
        .query_map([state.as_str()], read)
        .map_err(|err| format!("could not read sessions: {err}"))?;
    match rows.next() {
        None => Ok(None),
        Some(row) => row
            .map(Some)
            .map_err(|err| format!("could not read sessions: {err}")),
    }
}

/// The session that is counting down, if one is.
pub fn live(conn: &Connection) -> Result<Option<SessionRow>, String> {
    one(conn, State::Running)
}

/// The session whose question is owed, if one is.
pub fn awaiting(conn: &Connection) -> Result<Option<SessionRow>, String> {
    one(conn, State::AwaitingCheckin)
}

pub fn by_id(conn: &Connection, id: i64) -> Result<Option<SessionRow>, String> {
    let sql = format!("SELECT {COLUMNS} FROM sessions WHERE id = ?1");
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|err| format!("could not read session {id}: {err}"))?;
    let mut rows = stmt
        .query_map([id], read)
        .map_err(|err| format!("could not read session {id}: {err}"))?;
    match rows.next() {
        None => Ok(None),
        Some(row) => row
            .map(Some)
            .map_err(|err| format!("could not read session {id}: {err}")),
    }
}

pub fn mark(
    conn: &Connection,
    id: i64,
    state: State,
    ended_ts: Option<i64>,
) -> Result<(), String> {
    conn.execute(
        "UPDATE sessions SET state = ?2, ended_ts = COALESCE(?3, ended_ts) WHERE id = ?1",
        rusqlite::params![id, state.as_str(), ended_ts],
    )
    .map_err(|err| format!("could not update the session: {err}"))?;
    Ok(())
}

pub fn answer(
    conn: &Connection,
    id: i64,
    answer: Answer,
    task_done: bool,
    now: i64,
) -> Result<(), String> {
    conn.execute(
        "UPDATE sessions
         SET state = ?2, answered_ts = ?3, task_done = ?4, ended_ts = COALESCE(ended_ts, ?3)
         WHERE id = ?1",
        rusqlite::params![
            id,
            answer.into_state().as_str(),
            now,
            i64::from(task_done)
        ],
    )
    .map_err(|err| format!("could not record the answer: {err}"))?;
    Ok(())
}

/// Every session left open by a run that ended without closing it.
///
/// Rehydration must never trust "the row says running" — nothing closes a row
/// when the process is killed, so orphans are normal rather than exceptional.
pub fn open_sessions(conn: &Connection) -> Result<Vec<SessionRow>, String> {
    let sql = format!(
        "SELECT {COLUMNS} FROM sessions WHERE state IN ('running','awaiting_checkin') ORDER BY id"
    );
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|err| format!("could not read sessions: {err}"))?;
    let rows = stmt
        .query_map([], read)
        .map_err(|err| format!("could not read sessions: {err}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("could not read sessions: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory_db() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory database");
        crate::db::migrate_for_tests(&conn);
        conn
    }

    fn an_intention(conn: &Connection) -> i64 {
        conn.execute(
            "INSERT INTO intentions(ts, text, outcome) VALUES('now', 'write the report', 'completed')",
            [],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn a_session(conn: &Connection, minutes: i64, now: i64) -> i64 {
        let intention_id = an_intention(conn);
        start(
            conn,
            &NewSession {
                intention_id,
                task_id: None,
                task_title: Some("write the report".into()),
                duration_min: minutes,
                categories: Some("social".into()),
                predicted_yes: Some(true),
            },
            now,
        )
        .unwrap()
    }

    #[test]
    fn a_session_ends_a_duration_after_it_starts() {
        let conn = memory_db();
        let id = a_session(&conn, 25, 1_000);
        let row = by_id(&conn, id).unwrap().unwrap();
        assert_eq!(row.started_ts, 1_000);
        assert_eq!(row.ends_ts, 1_000 + 25 * 60);
        assert_eq!(row.state, State::Running);
        assert!(!row.enforced, "nothing is enforced until the helper confirms");
    }

    #[test]
    fn only_one_session_may_run_at_a_time() {
        // The guard is the partial unique index, not a check in one caller, so
        // that every future caller inherits it.
        let conn = memory_db();
        a_session(&conn, 25, 1_000);
        let intention_id = an_intention(&conn);
        let second = start(
            &conn,
            &NewSession {
                intention_id,
                task_id: None,
                task_title: None,
                duration_min: 25,
                categories: None,
                predicted_yes: None,
            },
            2_000,
        );
        assert!(second.is_err(), "a second running session must be refused");
    }

    #[test]
    fn ending_one_frees_the_slot() {
        let conn = memory_db();
        let first = a_session(&conn, 25, 1_000);
        mark(&conn, first, State::Lapsed, Some(2_500)).unwrap();
        assert!(a_session(&conn, 25, 3_000) > 0);
    }

    #[test]
    fn confirming_a_block_takes_the_helpers_end_time() {
        // The helper stamps its own clock seconds later; the session follows it
        // rather than counting to zero while the sites are still blocked.
        let conn = memory_db();
        let id = a_session(&conn, 25, 1_000);
        confirm_block(&conn, id, 1_000 + 25 * 60 + 9).unwrap();
        let row = by_id(&conn, id).unwrap().unwrap();
        assert!(row.enforced);
        assert_eq!(row.ends_ts, 1_000 + 25 * 60 + 9);
    }

    #[test]
    fn an_answer_is_recorded_with_its_state() {
        let conn = memory_db();
        let id = a_session(&conn, 25, 1_000);
        answer(&conn, id, Answer::Partly, true, 2_600).unwrap();
        let row = by_id(&conn, id).unwrap().unwrap();
        assert_eq!(row.state, State::Partly);
        assert_eq!(row.answered_ts, Some(2_600));
        assert!(row.task_done);
    }

    #[test]
    fn walking_away_is_not_the_same_as_saying_no() {
        // The distinction the check-in exists for: folding these together would
        // score a prediction against an answer nobody gave.
        let conn = memory_db();
        let a = a_session(&conn, 25, 1_000);
        mark(&conn, a, State::Unanswered, Some(2_500)).unwrap();
        let b = a_session(&conn, 25, 3_000);
        answer(&conn, b, Answer::No, false, 4_500).unwrap();

        assert_eq!(by_id(&conn, a).unwrap().unwrap().state, State::Unanswered);
        assert_eq!(by_id(&conn, b).unwrap().unwrap().state, State::Missed);
    }

    #[test]
    fn open_sessions_finds_what_a_killed_run_left_behind() {
        let conn = memory_db();
        let running = a_session(&conn, 25, 1_000);
        mark(&conn, running, State::AwaitingCheckin, Some(2_500)).unwrap();
        let second = a_session(&conn, 25, 3_000);

        let open = open_sessions(&conn).unwrap();
        assert_eq!(open.len(), 2);
        assert!(open.iter().any(|row| row.id == running));
        assert!(open.iter().any(|row| row.id == second));
    }
}
