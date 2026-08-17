use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// What the user did with a ritual. Every path through the popup writes one of
/// these — including the ones we would rather not see — because a record that
/// only counts successes cannot tell you anything true.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    /// Ritual finished and a session was committed to.
    Completed,
    /// Dismissed without answering.
    Skipped,
    /// Session was abandoned early.
    Drifted,
    /// The honourable exit: no session, no blocks, no guilt.
    Browsing,
}

impl Outcome {
    fn as_str(self) -> &'static str {
        match self {
            Outcome::Completed => "completed",
            Outcome::Skipped => "skipped",
            Outcome::Drifted => "drifted",
            Outcome::Browsing => "browsing",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewIntention {
    pub text: Option<String>,
    pub if_then: Option<String>,
    pub predicted_yes: Option<bool>,
    pub duration_min: Option<i64>,
    /// Block categories, comma separated. Empty means nothing was blocked.
    pub categories: Option<String>,
    pub trigger: Option<String>,
    pub outcome: Outcome,
    /// The task this intention came from, when it came from one.
    ///
    /// `serde(default)` so a frontend built before this field still starts a
    /// ritual rather than failing at the boundary. Recording the intention is
    /// the one thing that must not break.
    #[serde(default)]
    pub task_id: Option<i64>,
}

/// Read side is camelCase so the frontend does not have to translate; the write
/// side stays snake_case because that is what `toRust` in lib/tauri.ts sends.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentionRow {
    pub id: i64,
    pub ts: String,
    pub text: Option<String>,
    pub if_then: Option<String>,
    pub predicted_yes: Option<bool>,
    pub duration_min: Option<i64>,
    pub categories: Option<String>,
    pub trigger: Option<String>,
    pub outcome: String,
    pub task_id: Option<i64>,
}

pub fn insert(conn: &Connection, intention: &NewIntention) -> Result<i64, String> {
    let ts = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO intentions(ts, text, if_then, predicted_yes, duration_min, categories, trigger, outcome, task_id)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            ts,
            intention.text,
            intention.if_then,
            intention.predicted_yes.map(|yes| yes as i64),
            intention.duration_min,
            intention.categories,
            intention.trigger,
            intention.outcome.as_str(),
            intention.task_id,
        ],
    )
    .map_err(|err| format!("could not record intention: {err}"))?;

    Ok(conn.last_insert_rowid())
}

/// What one intention said, if it said anything. `None` for a missing row or
/// a blank text - both mean "nothing to name".
pub fn text_of(conn: &Connection, id: i64) -> Option<String> {
    conn.query_row(
        "SELECT text FROM intentions WHERE id = ?1",
        [id],
        |row| row.get::<_, Option<String>>(0),
    )
    .ok()
    .flatten()
    .map(|text| text.trim().to_owned())
    .filter(|text| !text.is_empty())
}

pub fn recent(conn: &Connection, limit: i64) -> Result<Vec<IntentionRow>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, ts, text, if_then, predicted_yes, duration_min, categories, trigger, outcome, task_id
             FROM intentions ORDER BY ts DESC LIMIT ?1",
        )
        .map_err(|err| format!("could not read intentions: {err}"))?;

    let rows = stmt
        .query_map([limit], |row| {
            Ok(IntentionRow {
                id: row.get(0)?,
                ts: row.get(1)?,
                text: row.get(2)?,
                if_then: row.get(3)?,
                predicted_yes: row.get::<_, Option<i64>>(4)?.map(|v| v != 0),
                duration_min: row.get(5)?,
                categories: row.get(6)?,
                trigger: row.get(7)?,
                outcome: row.get(8)?,
                task_id: row.get(9)?,
            })
        })
        .map_err(|err| format!("could not read intentions: {err}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("could not read intentions: {err}"))
}

/// Distinct recent intention texts, most recent first.
///
/// Until the Eisenhower matrix exists these are what the ritual offers as
/// one-click chips — the spec's fallback when the "Do First" quadrant is empty.
pub fn recent_texts(conn: &Connection, limit: i64) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT text FROM intentions
             WHERE text IS NOT NULL AND TRIM(text) <> ''
             GROUP BY text ORDER BY MAX(ts) DESC LIMIT ?1",
        )
        .map_err(|err| format!("could not read recent intentions: {err}"))?;

    let rows = stmt
        .query_map([limit], |row| row.get::<_, String>(0))
        .map_err(|err| format!("could not read recent intentions: {err}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("could not read recent intentions: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory_db() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory database");
        crate::db::migrate_for_tests(&conn);
        conn
    }

    fn sample(outcome: Outcome, text: &str) -> NewIntention {
        NewIntention {
            text: Some(text.to_string()),
            if_then: Some("take one breath and return to my task".into()),
            predicted_yes: Some(true),
            duration_min: Some(25),
            categories: Some("social,video".into()),
            trigger: Some("boot".into()),
            outcome,
            task_id: None,
        }
    }

    #[test]
    fn an_intention_can_be_asked_what_it_was_for() {
        let conn = memory_db();
        let id = insert(&conn, &sample(Outcome::Completed, "  write the report ")).unwrap();
        assert_eq!(text_of(&conn, id), Some("write the report".to_string()));
        let mut blank = sample(Outcome::Completed, "   ");
        blank.text = Some("   ".into());
        let blank_id = insert(&conn, &blank).unwrap();
        assert_eq!(text_of(&conn, blank_id), None, "blank is nothing, not an empty name");
        assert_eq!(text_of(&conn, 9999), None);
    }

    #[test]
    fn records_every_outcome_including_the_unflattering_ones() {
        let conn = memory_db();
        for (outcome, text) in [
            (Outcome::Completed, "write the report"),
            (Outcome::Skipped, "skipped one"),
            (Outcome::Drifted, "drifted one"),
            (Outcome::Browsing, "browsing one"),
        ] {
            insert(&conn, &sample(outcome, text)).expect("insert");
        }
        assert_eq!(recent(&conn, 10).unwrap().len(), 4);
    }

    #[test]
    fn round_trips_the_fields_the_ritual_collects() {
        let conn = memory_db();
        insert(&conn, &sample(Outcome::Completed, "write the report")).unwrap();
        let row = &recent(&conn, 1).unwrap()[0];
        assert_eq!(row.text.as_deref(), Some("write the report"));
        assert_eq!(row.predicted_yes, Some(true));
        assert_eq!(row.duration_min, Some(25));
        assert_eq!(row.outcome, "completed");
        assert_eq!(row.trigger.as_deref(), Some("boot"));
    }

    #[test]
    fn a_no_prediction_is_stored_as_false_not_as_missing() {
        let conn = memory_db();
        let mut intention = sample(Outcome::Completed, "x");
        intention.predicted_yes = Some(false);
        insert(&conn, &intention).unwrap();
        assert_eq!(recent(&conn, 1).unwrap()[0].predicted_yes, Some(false));
    }

    #[test]
    fn a_task_link_is_written_and_read_back() {
        // `intentions.task_id` shipped in migration 2 and was then never written
        // and never read - the app could not say which task a session was for.
        // This is the test that keeps it alive.
        let conn = memory_db();
        conn.execute(
            "INSERT INTO tasks(id, title, status) VALUES(7, 'create stripe account', 'open')",
            [],
        )
        .unwrap();

        let mut intention = sample(Outcome::Completed, "create stripe account");
        intention.task_id = Some(7);
        insert(&conn, &intention).unwrap();

        assert_eq!(recent(&conn, 1).unwrap()[0].task_id, Some(7));
    }

    #[test]
    fn an_intention_without_a_task_is_still_recorded() {
        // Typed from scratch at boot, or picked from history: most intentions
        // have no task behind them and must not require one.
        let conn = memory_db();
        insert(&conn, &sample(Outcome::Completed, "think")).unwrap();
        assert_eq!(recent(&conn, 1).unwrap()[0].task_id, None);
    }

    #[test]
    fn the_honourable_exit_records_no_duration_and_no_blocks() {
        let conn = memory_db();
        let intention = NewIntention {
            text: None,
            if_then: None,
            predicted_yes: Some(false),
            duration_min: None,
            categories: None,
            trigger: Some("wake".into()),
            outcome: Outcome::Browsing,
            task_id: None,
        };
        insert(&conn, &intention).unwrap();
        let row = &recent(&conn, 1).unwrap()[0];
        assert_eq!(row.outcome, "browsing");
        assert_eq!(row.duration_min, None);
        assert_eq!(row.categories, None);
    }

    #[test]
    fn recent_texts_deduplicates_and_orders_by_last_use() {
        let conn = memory_db();
        insert(&conn, &sample(Outcome::Completed, "older")).unwrap();
        insert(&conn, &sample(Outcome::Completed, "repeated")).unwrap();
        insert(&conn, &sample(Outcome::Completed, "repeated")).unwrap();
        let texts = recent_texts(&conn, 10).unwrap();
        assert_eq!(texts.len(), 2);
        assert!(texts.contains(&"repeated".to_string()));
        assert!(texts.contains(&"older".to_string()));
    }

    #[test]
    fn an_invalid_outcome_is_rejected_by_the_schema() {
        let conn = memory_db();
        let result = conn.execute(
            "INSERT INTO intentions(ts, outcome) VALUES('2026-01-01T00:00:00Z', 'procrastinated')",
            [],
        );
        assert!(result.is_err(), "schema must reject unknown outcomes");
    }
}
