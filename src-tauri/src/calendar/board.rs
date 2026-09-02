//! Channel 2: the whole board, synced through Cloud Firestore.
//!
//! The calendar (channel 1) carries only Schedule's events; this carries
//! everything - every quadrant, every area, dated or not - as one document
//! per task at `boards/main/tasks/{uid}` in a Firestore database inside the
//! user's own Cloud project. The mobile app reads and writes the same tree
//! through the Firebase SDK; the desktop speaks the REST API with the same
//! OAuth token the calendar already holds (plus the `datastore` scope).
//!
//! The wire schema is HANDOFF §4.3 in the threshold-mobile repo - field
//! names, quadrant spellings, and the conflict rule are fixed there. Conflict
//! rule: whole-document last-writer-wins on `updatedTs` (unix seconds).
//! Tombstones (`status: "deleted"`) are kept 30 days, then any device that
//! notices deletes the document for real.
//!
//! Same threading discipline as the rest of the module: never hold the DB
//! lock across an HTTP call, and never let a board failure touch a task move
//! - a sync error is a status line, not a gate.

use rusqlite::OptionalExtension;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use super::api::Client;
use crate::db::{self, tasks, Db};

/// The user's GCP project - the same one that holds their OAuth clients and
/// the mobile app's Firebase. Overridable via the `board_project_id` setting;
/// a project id is an identifier, not a secret (it rides in every consent
/// URL), so a default here breaks no rule the calendar setup didn't already.
const DEFAULT_PROJECT: &str = "jovial-world-505914-t4";

/// Tombstones older than this are erased for real.
const TOMBSTONE_KEEP_SECS: i64 = 30 * 24 * 3600;

const SCHEMA_V: i64 = 1;

fn docs_base(project: &str) -> String {
    format!("https://firestore.googleapis.com/v1/projects/{project}/databases/(default)/documents")
}

fn project(app: &AppHandle) -> String {
    let db = app.state::<Db>();
    db.0.lock()
        .ok()
        .and_then(|conn| db::get_setting(&conn, "board_project_id"))
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_PROJECT.into())
}

// ---- the wire codec ------------------------------------------------------

fn quadrant_wire(urgent: Option<bool>, important: Option<bool>) -> &'static str {
    match (urgent, important) {
        (Some(true), Some(true)) => "do_first",
        (Some(false), Some(true)) => "schedule",
        (Some(true), Some(false)) => "delegate",
        (Some(false), Some(false)) => "eliminate",
        _ => "inbox",
    }
}

fn quadrant_flags(wire: &str) -> (Option<bool>, Option<bool>) {
    match wire {
        "do_first" => (Some(true), Some(true)),
        "schedule" => (Some(false), Some(true)),
        "delegate" => (Some(true), Some(false)),
        "eliminate" => (Some(false), Some(false)),
        _ => (None, None),
    }
}

fn sv(s: &str) -> Value {
    json!({ "stringValue": s })
}
fn iv(n: i64) -> Value {
    json!({ "integerValue": n.to_string() })
}

fn rfc3339_to_epoch(s: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.timestamp())
}

fn epoch_to_rfc3339(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_default()
}

/// One dirty local row, joined to its area name, ready to encode.
struct DirtyRow {
    id: i64,
    uid: String,
    title: String,
    note: Option<String>,
    area: Option<String>,
    urgent: Option<bool>,
    important: Option<bool>,
    sort_order: i64,
    status: String,
    created_ts: Option<String>,
    completed_ts: Option<String>,
    scheduled_ts: Option<i64>,
    repeat_days: Option<String>,
    updated_ts: i64,
}

fn encode_row(row: &DirtyRow) -> Value {
    let mut fields = serde_json::Map::new();
    fields.insert("schemaV".into(), iv(SCHEMA_V));
    fields.insert("title".into(), sv(&row.title));
    if let Some(note) = &row.note {
        fields.insert("note".into(), sv(note));
    }
    fields.insert(
        "quadrant".into(),
        sv(quadrant_wire(row.urgent, row.important)),
    );
    fields.insert("status".into(), sv(&row.status));
    fields.insert("sortOrder".into(), iv(row.sort_order));
    if let Some(area) = &row.area {
        fields.insert("area".into(), sv(area));
    }
    if let Some(days) = &row.repeat_days {
        fields.insert("repeatDays".into(), sv(days));
    }
    if let Some(ts) = row.scheduled_ts {
        fields.insert("scheduledTs".into(), iv(ts));
    }
    let created = row
        .created_ts
        .as_deref()
        .and_then(rfc3339_to_epoch)
        .unwrap_or(row.updated_ts);
    fields.insert("createdTs".into(), iv(created));
    if let Some(ts) = row.completed_ts.as_deref().and_then(rfc3339_to_epoch) {
        fields.insert("completedTs".into(), iv(ts));
    }
    fields.insert("updatedTs".into(), iv(row.updated_ts));
    fields.insert("legacyDesktopId".into(), iv(row.id));
    json!({ "fields": fields })
}

/// A task document as it arrives. Absent and null fields read the same.
struct RemoteDoc {
    uid: String,
    title: String,
    note: Option<String>,
    urgent: Option<bool>,
    important: Option<bool>,
    status: String,
    sort_order: i64,
    area: Option<String>,
    repeat_days: Option<String>,
    scheduled_ts: Option<i64>,
    created_ts: i64,
    completed_ts: Option<i64>,
    updated_ts: i64,
}

fn field_str(fields: &Value, name: &str) -> Option<String> {
    fields
        .get(name)?
        .get("stringValue")?
        .as_str()
        .map(String::from)
}

fn field_int(fields: &Value, name: &str) -> Option<i64> {
    fields
        .get(name)?
        .get("integerValue")?
        .as_str()?
        .parse()
        .ok()
}

fn decode_doc(doc: &Value) -> Option<RemoteDoc> {
    let uid = doc
        .get("name")?
        .as_str()?
        .rsplit('/')
        .next()?
        .to_string();
    let fields = doc.get("fields")?;
    let updated_ts = field_int(fields, "updatedTs")?;
    let (urgent, important) =
        quadrant_flags(&field_str(fields, "quadrant").unwrap_or_default());
    let status = field_str(fields, "status").unwrap_or_else(|| "open".into());
    // Statuses outside the desktop's CHECK constraint would fail the INSERT;
    // an unknown spelling from a future schema reads as open, not as a crash.
    let status = if matches!(status.as_str(), "open" | "done" | "archived" | "deleted") {
        status
    } else {
        "open".into()
    };
    Some(RemoteDoc {
        uid,
        title: field_str(fields, "title")?,
        note: field_str(fields, "note"),
        urgent,
        important,
        status,
        sort_order: field_int(fields, "sortOrder").unwrap_or(0),
        area: field_str(fields, "area"),
        repeat_days: field_str(fields, "repeatDays"),
        scheduled_ts: field_int(fields, "scheduledTs"),
        created_ts: field_int(fields, "createdTs").unwrap_or(updated_ts),
        completed_ts: field_int(fields, "completedTs"),
        updated_ts,
    })
}

// ---- HTTP ----------------------------------------------------------------

/// Send a Firestore request, mapping the two statuses that mean something
/// specific: 403 is the missing `datastore` scope (a token from before the
/// board channel), everything else is an ordinary error string.
fn send(req: reqwest::blocking::RequestBuilder, what: &str) -> Result<Value, String> {
    let resp = req
        .send()
        .map_err(|err| format!("{what} failed: {err}"))?;
    if resp.status().as_u16() == 403 {
        return Err("BOARD_SCOPE".into());
    }
    let resp = resp
        .error_for_status()
        .map_err(|err| format!("{what} rejected: {err}"))?;
    resp.json()
        .map_err(|err| format!("{what} response unreadable: {err}"))
}

// ---- push ----------------------------------------------------------------

fn push_dirty(app: &AppHandle, client: &Client, base: &str) -> Result<(), String> {
    let rows: Vec<DirtyRow> = {
        let db = app.state::<Db>();
        let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
        let mut stmt = conn
            .prepare(
                "SELECT t.id, t.uid, t.title, t.note, c.name, t.urgent, t.important,
                        t.sort_order, t.status, t.created_ts, t.completed_ts,
                        t.scheduled_ts, t.repeat_days, t.updated_ts
                 FROM tasks t LEFT JOIN contexts c ON c.id = t.context_id
                 WHERE t.board_dirty = 1 AND t.uid IS NOT NULL",
            )
            .map_err(|err| format!("could not read dirty tasks: {err}"))?;
        let mapped = stmt
            .query_map([], |row| {
                Ok(DirtyRow {
                    id: row.get(0)?,
                    uid: row.get(1)?,
                    title: row.get(2)?,
                    note: row.get(3)?,
                    area: row.get(4)?,
                    urgent: row.get::<_, Option<i64>>(5)?.map(|v| v != 0),
                    important: row.get::<_, Option<i64>>(6)?.map(|v| v != 0),
                    sort_order: row.get(7)?,
                    status: row.get(8)?,
                    created_ts: row.get(9)?,
                    completed_ts: row.get(10)?,
                    scheduled_ts: row.get(11)?,
                    repeat_days: row.get(12)?,
                    updated_ts: row.get(13)?,
                })
            })
            .map_err(|err| format!("could not read dirty tasks: {err}"))?;
        mapped
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("could not read dirty tasks: {err}"))?
    };

    for row in &rows {
        let url = format!("{base}/boards/main/tasks/{}", row.uid);
        send(
            client
                .http
                .patch(&url)
                .bearer_auth(&client.access)
                .json(&encode_row(row)),
            "board push",
        )?;
        // Clear the mark only if the row did not change again mid-flight.
        let db = app.state::<Db>();
        let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
        let _ = conn.execute(
            "UPDATE tasks SET board_dirty = 0 WHERE id = ?1 AND updated_ts = ?2",
            rusqlite::params![row.id, row.updated_ts],
        );
    }

    // The areas list rides on the board's one meta document.
    let dirty_areas = {
        let db = app.state::<Db>();
        let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
        db::get_setting(&conn, "board_areas_dirty").as_deref() == Some("1")
    };
    if dirty_areas {
        let (areas, now) = {
            let db = app.state::<Db>();
            let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
            (tasks::contexts(&conn)?, chrono::Utc::now().timestamp())
        };
        let values: Vec<Value> = areas
            .iter()
            .map(|a| {
                json!({ "mapValue": { "fields": {
                    "name": sv(&a.name),
                    "sortOrder": iv(a.sort_order),
                }}})
            })
            .collect();
        let body = json!({ "fields": {
            "schemaV": iv(SCHEMA_V),
            "areas": { "arrayValue": { "values": values } },
            "updatedTs": iv(now),
        }});
        // updateMask, not full replace: quotes share this document under
        // their own clock, and an unmasked PATCH would erase them.
        send(
            client
                .http
                .patch(format!(
                    "{base}/boards/main?updateMask.fieldPaths=schemaV\
                     &updateMask.fieldPaths=areas&updateMask.fieldPaths=updatedTs"
                ))
                .bearer_auth(&client.access)
                .json(&body),
            "areas push",
        )?;
        let db = app.state::<Db>();
        let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
        let _ = db::set_setting(&conn, "board_areas_ts", &now.to_string());
        let _ = db::set_setting(&conn, "board_areas_dirty", "0");
    }

    // The quote reservoir, on its own clock.
    let quotes_dirty = {
        let db = app.state::<Db>();
        let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
        db::get_setting(&conn, "board_quotes_dirty").as_deref() == Some("1")
    };
    if quotes_dirty {
        let (rows, clock) = {
            let db = app.state::<Db>();
            let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
            let mut stmt = conn
                .prepare("SELECT text, author, created_ts FROM quotes ORDER BY id")
                .map_err(|err| format!("could not read quotes: {err}"))?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                })
                .map_err(|err| format!("could not read quotes: {err}"))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|err| format!("could not read quotes: {err}"))?;
            let clock = db::get_setting(&conn, "quotes_updated_ts")
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or_else(|| chrono::Utc::now().timestamp());
            (rows, clock)
        };
        let values: Vec<Value> = rows
            .iter()
            .map(|(text, author, created)| {
                let mut fields = serde_json::Map::new();
                fields.insert("text".into(), sv(text));
                if let Some(author) = author {
                    fields.insert("author".into(), sv(author));
                }
                fields.insert(
                    "createdTs".into(),
                    sv(created.as_deref().unwrap_or("")),
                );
                json!({ "mapValue": { "fields": fields } })
            })
            .collect();
        let body = json!({ "fields": {
            "quotes": { "arrayValue": { "values": values } },
            "quotesUpdatedTs": iv(clock),
        }});
        send(
            client
                .http
                .patch(format!(
                    "{base}/boards/main?updateMask.fieldPaths=quotes\
                     &updateMask.fieldPaths=quotesUpdatedTs"
                ))
                .bearer_auth(&client.access)
                .json(&body),
            "quotes push",
        )?;
        let db = app.state::<Db>();
        let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
        let _ = db::set_setting(&conn, "board_quotes_dirty", "0");
    }
    Ok(())
}

// ---- pull ----------------------------------------------------------------

/// Find an area id by name, creating the area when there is room. A board
/// full of areas drops the label rather than failing the task.
fn resolve_area(conn: &rusqlite::Connection, name: Option<&str>) -> Option<i64> {
    let name = name?.trim();
    if name.is_empty() {
        return None;
    }
    let found: Option<i64> = conn
        .query_row(
            "SELECT id FROM contexts WHERE name = ?1 COLLATE NOCASE",
            [name],
            |r| r.get(0),
        )
        .optional()
        .ok()
        .flatten();
    if found.is_some() {
        return found;
    }
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM contexts", [], |r| r.get(0))
        .unwrap_or(0);
    if count >= tasks::MAX_CONTEXTS as i64 {
        return None;
    }
    conn.execute(
        "INSERT INTO contexts(name, sort_order)
         VALUES(?1, (SELECT COALESCE(MAX(sort_order) + 1, 0) FROM contexts))",
        [name],
    )
    .ok()?;
    Some(conn.last_insert_rowid())
}

/// Apply one remote document under last-writer-wins. Returns
/// `(changed, event_to_delete)` - the delete happens after the lock drops.
fn apply_doc(app: &AppHandle, doc: &RemoteDoc) -> Result<(bool, Option<String>), String> {
    let db = app.state::<Db>();
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;

    let local: Option<(i64, i64, Option<String>)> = conn
        .query_row(
            "SELECT id, updated_ts, calendar_event_id FROM tasks WHERE uid = ?1",
            [&doc.uid],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(|err| format!("could not read task {}: {err}", doc.uid))?;

    let context_id = resolve_area(&conn, doc.area.as_deref());
    let in_schedule_remote = doc.urgent == Some(false)
        && doc.important == Some(true)
        && doc.status == "open";

    match local {
        None => {
            if doc.status == "deleted" {
                return Ok((false, None)); // a tombstone for a task we never knew
            }
            conn.execute(
                "INSERT INTO tasks(title, note, context_id, urgent, important, sort_order,
                                   status, created_ts, completed_ts, scheduled_ts, repeat_days,
                                   uid, updated_ts, board_dirty)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 0)",
                rusqlite::params![
                    doc.title,
                    doc.note,
                    context_id,
                    doc.urgent.map(|v| v as i64),
                    doc.important.map(|v| v as i64),
                    doc.sort_order,
                    doc.status,
                    epoch_to_rfc3339(doc.created_ts),
                    doc.completed_ts.map(epoch_to_rfc3339),
                    doc.scheduled_ts,
                    doc.repeat_days,
                    doc.uid,
                    doc.updated_ts,
                ],
            )
            .map_err(|err| format!("could not adopt task \"{}\": {err}", doc.title))?;
            Ok((true, None))
        }
        Some((id, local_ts, _)) if doc.updated_ts <= local_ts => {
            let _ = id;
            Ok((false, None))
        }
        Some((id, _, event_id)) => {
            // We hold a channel-1 event for this task. While it stays open in
            // Schedule the link - and the slot channel 1 owns - is preserved;
            // the moment the remote record leaves, the event goes with it
            // (the other device cannot delete what this install created).
            let keep_link = event_id.is_some() && in_schedule_remote;
            let to_delete = if keep_link { None } else { event_id };
            if keep_link {
                conn.execute(
                    "UPDATE tasks SET title = ?1, note = ?2, context_id = ?3, urgent = ?4,
                            important = ?5, sort_order = ?6, status = ?7, completed_ts = ?8,
                            repeat_days = ?9, updated_ts = ?10, board_dirty = 0
                     WHERE id = ?11",
                    rusqlite::params![
                        doc.title,
                        doc.note,
                        context_id,
                        doc.urgent.map(|v| v as i64),
                        doc.important.map(|v| v as i64),
                        doc.sort_order,
                        doc.status,
                        doc.completed_ts.map(epoch_to_rfc3339),
                        doc.repeat_days,
                        doc.updated_ts,
                        id,
                    ],
                )
            } else {
                conn.execute(
                    "UPDATE tasks SET title = ?1, note = ?2, context_id = ?3, urgent = ?4,
                            important = ?5, sort_order = ?6, status = ?7, completed_ts = ?8,
                            repeat_days = ?9, updated_ts = ?10, board_dirty = 0,
                            scheduled_ts = ?12, calendar_event_id = NULL,
                            calendar_html_link = NULL, remind_fired_for_ts = NULL,
                            remind_snoozed_until = NULL
                     WHERE id = ?11",
                    rusqlite::params![
                        doc.title,
                        doc.note,
                        context_id,
                        doc.urgent.map(|v| v as i64),
                        doc.important.map(|v| v as i64),
                        doc.sort_order,
                        doc.status,
                        doc.completed_ts.map(epoch_to_rfc3339),
                        doc.repeat_days,
                        doc.updated_ts,
                        id,
                        doc.scheduled_ts,
                    ],
                )
            }
            .map_err(|err| format!("could not apply \"{}\": {err}", doc.title))?;
            Ok((true, to_delete))
        }
    }
}

fn apply_areas(app: &AppHandle, remote: &[(String, i64)], remote_ts: i64) -> Result<(), String> {
    let db = app.state::<Db>();
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    let local = tasks::contexts(&conn)?;
    for (name, sort_order) in remote {
        match local.iter().find(|c| c.name.eq_ignore_ascii_case(name)) {
            Some(c) => {
                if c.sort_order != *sort_order {
                    let _ = conn.execute(
                        "UPDATE contexts SET sort_order = ?1 WHERE id = ?2",
                        rusqlite::params![sort_order, c.id],
                    );
                }
            }
            None => {
                if local.len() < tasks::MAX_CONTEXTS {
                    let _ = conn.execute(
                        "INSERT INTO contexts(name, sort_order) VALUES(?1, ?2)",
                        rusqlite::params![name, sort_order],
                    );
                }
            }
        }
    }
    // An area the other device removed: tasks keep everything but the label,
    // the desktop's own remove rule.
    for c in &local {
        if !remote.iter().any(|(name, _)| name.eq_ignore_ascii_case(&c.name)) {
            let _ = conn.execute(
                "UPDATE tasks SET context_id = NULL WHERE context_id = ?1",
                [c.id],
            );
            let _ = conn.execute("DELETE FROM contexts WHERE id = ?1", [c.id]);
        }
    }
    // The applies above fired the dirty trigger; this was a remote change,
    // not ours, so the mark comes straight back off.
    let _ = db::set_setting(&conn, "board_areas_ts", &remote_ts.to_string());
    let _ = db::set_setting(&conn, "board_areas_dirty", "0");
    Ok(())
}

fn pull_delta(app: &AppHandle, client: &Client, base: &str) -> Result<bool, String> {
    let last: i64 = {
        let db = app.state::<Db>();
        let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
        db::get_setting(&conn, "board_pull_ts")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    };

    let body = json!({ "structuredQuery": {
        "from": [{ "collectionId": "tasks" }],
        "where": { "fieldFilter": {
            "field": { "fieldPath": "updatedTs" },
            "op": "GREATER_THAN",
            "value": { "integerValue": last.to_string() },
        }},
    }});
    let resp = send(
        client
            .http
            .post(format!("{base}/boards/main:runQuery"))
            .bearer_auth(&client.access)
            .json(&body),
        "board pull",
    )?;

    let mut changed = false;
    let mut max_ts = last;
    let mut events_to_delete = Vec::new();
    let mut tombstones_to_gc = Vec::new();
    let now = chrono::Utc::now().timestamp();

    if let Some(lines) = resp.as_array() {
        for line in lines {
            let Some(doc) = line.get("document") else { continue };
            let Some(decoded) = decode_doc(doc) else { continue };
            max_ts = max_ts.max(decoded.updated_ts);
            if decoded.status == "deleted"
                && now - decoded.updated_ts > TOMBSTONE_KEEP_SECS
            {
                tombstones_to_gc.push(decoded.uid.clone());
            }
            let (did, event) = apply_doc(app, &decoded)?;
            changed |= did;
            if let Some(id) = event {
                events_to_delete.push(id);
            }
        }
    }

    // Cleanup outside the lock: lingering channel-1 events for tasks the
    // other device closed, and tombstones past their keep window.
    for id in events_to_delete {
        let _ = client.delete_event(&id);
    }
    for uid in tombstones_to_gc {
        let _ = send(
            client
                .http
                .delete(format!("{base}/boards/main/tasks/{uid}"))
                .bearer_auth(&client.access),
            "tombstone gc",
        );
    }

    {
        let db = app.state::<Db>();
        let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
        let _ = db::set_setting(&conn, "board_pull_ts", &max_ts.to_string());
    }

    // The areas meta document, LWW on its own clock.
    let meta = send(
        client
            .http
            .get(format!("{base}/boards/main"))
            .bearer_auth(&client.access),
        "areas pull",
    );
    if let Ok(meta) = meta {
        if let Some(fields) = meta.get("fields") {
            let remote_ts = field_int(fields, "updatedTs").unwrap_or(0);
            let local_ts: i64 = {
                let db = app.state::<Db>();
                let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
                db::get_setting(&conn, "board_areas_ts")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0)
            };
            if remote_ts > local_ts {
                let mut areas = Vec::new();
                if let Some(values) = fields
                    .get("areas")
                    .and_then(|a| a.get("arrayValue"))
                    .and_then(|a| a.get("values"))
                    .and_then(|v| v.as_array())
                {
                    for v in values {
                        if let Some(f) = v.get("mapValue").and_then(|m| m.get("fields")) {
                            if let Some(name) = field_str(f, "name") {
                                areas.push((name, field_int(f, "sortOrder").unwrap_or(0)));
                            }
                        }
                    }
                }
                apply_areas(app, &areas, remote_ts)?;
                changed = true;
            }
            // The quote reservoir, LWW on its own clock. Whole-list: the
            // newer reservoir replaces the older one.
            let remote_quotes_ts = field_int(fields, "quotesUpdatedTs").unwrap_or(0);
            let local_quotes_ts: i64 = {
                let db = app.state::<Db>();
                let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
                db::get_setting(&conn, "quotes_updated_ts")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0)
            };
            if remote_quotes_ts > local_quotes_ts {
                let mut incoming = Vec::new();
                if let Some(values) = fields
                    .get("quotes")
                    .and_then(|a| a.get("arrayValue"))
                    .and_then(|a| a.get("values"))
                    .and_then(|v| v.as_array())
                {
                    for v in values {
                        if let Some(f) = v.get("mapValue").and_then(|m| m.get("fields")) {
                            if let Some(text) = field_str(f, "text") {
                                incoming.push((
                                    text,
                                    field_str(f, "author"),
                                    field_str(f, "createdTs")
                                        .filter(|s| !s.is_empty()),
                                ));
                            }
                        }
                    }
                }
                let db = app.state::<Db>();
                let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
                let _ = conn.execute("DELETE FROM quotes", []);
                for (text, author, created) in &incoming {
                    let _ = conn.execute(
                        "INSERT OR IGNORE INTO quotes(text, author, created_ts)
                         VALUES(?1, ?2, COALESCE(?3, datetime('now')))",
                        rusqlite::params![text, author, created],
                    );
                }
                // The applies above fired the dirty trigger; this change was
                // remote, so both marks come straight back off.
                let _ = db::set_setting(
                    &conn,
                    "quotes_updated_ts",
                    &remote_quotes_ts.to_string(),
                );
                let _ = db::set_setting(&conn, "board_quotes_dirty", "0");
                drop(conn);
                // An open Settings window renders the reservoir it fetched
                // at mount; without this it shows yesterday's list while
                // the database already moved on.
                let _ = app.emit("quotes-changed", ());
            }
        }
    }

    Ok(changed)
}

// ---- the pass ------------------------------------------------------------

/// One board pass: push what changed here, pull what changed elsewhere.
/// Push first, so an edit made on this machine seconds ago wins the race
/// against a stale remote copy. Returns whether local rows changed.
pub fn sync_board(app: &AppHandle, client: &Client) -> Result<bool, String> {
    let base = docs_base(&project(app));
    push_dirty(app, client, &base)?;
    pull_delta(app, client, &base)
}

/// The uid a task carries, for stamping onto its calendar events.
pub fn uid_of(conn: &rusqlite::Connection, task_id: i64) -> Option<String> {
    conn.query_row("SELECT uid FROM tasks WHERE id = ?1", [task_id], |r| {
        r.get(0)
    })
    .optional()
    .ok()
    .flatten()
}

/// Resolve which local task an event belongs to. The uid is authoritative -
/// rowids collide across devices, so a legacy id only counts when the event
/// carries no uid at all (a desktop event from before the board channel).
pub fn resolve_event_task(
    conn: &rusqlite::Connection,
    task_id: Option<i64>,
    task_uid: Option<&str>,
) -> Option<i64> {
    if let Some(uid) = task_uid {
        return conn
            .query_row("SELECT id FROM tasks WHERE uid = ?1", [uid], |r| r.get(0))
            .optional()
            .ok()
            .flatten();
    }
    task_id
}

#[cfg(test)]
mod codec_tests {
    use super::*;

    #[test]
    fn quadrants_round_trip_the_wire_spellings() {
        for (u, i, wire) in [
            (None, None, "inbox"),
            (Some(true), Some(true), "do_first"),
            (Some(false), Some(true), "schedule"),
            (Some(true), Some(false), "delegate"),
            (Some(false), Some(false), "eliminate"),
        ] {
            assert_eq!(quadrant_wire(u, i), wire);
            assert_eq!(quadrant_flags(wire), (u, i));
        }
        assert_eq!(quadrant_flags("nonsense"), (None, None));
    }

    #[test]
    fn an_encoded_row_decodes_back_field_for_field() {
        let row = DirtyRow {
            id: 42,
            uid: "u-1".into(),
            title: "Ship the board".into(),
            note: Some("carefully".into()),
            area: Some("Job".into()),
            urgent: Some(false),
            important: Some(true),
            sort_order: 3,
            status: "open".into(),
            created_ts: Some("2026-08-31T10:00:00+05:00".into()),
            completed_ts: None,
            scheduled_ts: Some(1_788_300_000),
            repeat_days: Some("1,3,5".into()),
            updated_ts: 1_788_250_000,
        };
        let encoded = encode_row(&row);
        let doc = json!({
            "name": "projects/p/databases/(default)/documents/boards/main/tasks/u-1",
            "fields": encoded["fields"],
        });
        let decoded = decode_doc(&doc).expect("decodes");
        assert_eq!(decoded.uid, "u-1");
        assert_eq!(decoded.title, "Ship the board");
        assert_eq!(decoded.note.as_deref(), Some("carefully"));
        assert_eq!((decoded.urgent, decoded.important), (Some(false), Some(true)));
        assert_eq!(decoded.status, "open");
        assert_eq!(decoded.sort_order, 3);
        assert_eq!(decoded.area.as_deref(), Some("Job"));
        assert_eq!(decoded.repeat_days.as_deref(), Some("1,3,5"));
        assert_eq!(decoded.scheduled_ts, Some(1_788_300_000));
        assert_eq!(decoded.completed_ts, None);
        assert_eq!(decoded.updated_ts, 1_788_250_000);
        // The rfc3339 timestamp became the epoch second it names.
        assert_eq!(decoded.created_ts, 1_788_152_400);
    }

    #[test]
    fn an_unparseable_created_ts_falls_back_to_the_clock() {
        let row = DirtyRow {
            id: 1,
            uid: "u".into(),
            title: "t".into(),
            note: None,
            area: None,
            urgent: None,
            important: None,
            sort_order: 0,
            status: "open".into(),
            created_ts: Some("then".into()), // the old test fixtures' spelling
            completed_ts: None,
            scheduled_ts: None,
            repeat_days: None,
            updated_ts: 99,
        };
        let encoded = encode_row(&row);
        assert_eq!(
            encoded["fields"]["createdTs"]["integerValue"].as_str(),
            Some("99")
        );
    }

    #[test]
    fn a_malformed_document_reads_as_nothing_not_a_crash() {
        assert!(decode_doc(&json!({})).is_none());
        assert!(decode_doc(&json!({
            "name": "x/tasks/u",
            "fields": { "title": { "stringValue": "no clock" } },
        }))
        .is_none());
        // An unknown status coerces to open rather than violating the CHECK.
        let doc = decode_doc(&json!({
            "name": "x/tasks/u",
            "fields": {
                "title": { "stringValue": "t" },
                "updatedTs": { "integerValue": "5" },
                "status": { "stringValue": "paused" },
            },
        }))
        .expect("decodes");
        assert_eq!(doc.status, "open");
    }
}
