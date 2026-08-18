//! The Google Calendar calls the feature actually makes.
//!
//! One primary calendar, four verbs: read free/busy, create an event, move an
//! event, delete an event - plus the list-with-sync-token that lets a poll see
//! what the user changed in Google. Every call takes a fresh access token from
//! `Client::authed`, which refreshes transparently when the old one is close to
//! expiry. Nothing here touches the database except to read/write tokens, and
//! never while an HTTP call is in flight.

use chrono::{DateTime, Local, TimeZone, Utc};
use tauri::{AppHandle, Manager};

use super::oauth;
use super::token;
use crate::db::{self, Db};

const BASE: &str = "https://www.googleapis.com/calendar/v3";

/// A busy interval in local time.
pub type Busy = (DateTime<Local>, DateTime<Local>);

pub struct Client {
    http: reqwest::blocking::Client,
    access: String,
}

/// What `insert_event` hands back: enough to record the link on the task.
pub struct EventRef {
    pub id: String,
    pub html_link: Option<String>,
}

/// One event as a poll sees it.
pub struct EventItem {
    pub id: String,
    pub task_id: Option<i64>,
    pub cancelled: bool,
    pub start_ts: Option<i64>,
}

impl Client {
    /// A client carrying a valid access token, refreshing first if the stored
    /// one is within a minute of expiry.
    pub fn authed(app: &AppHandle) -> Result<Client, String> {
        let tokens = {
            let db = app.state::<Db>();
            let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
            token::load(&conn)
        };
        let tokens = tokens.ok_or("Not connected to Google Calendar.")?;
        let tokens = if tokens.expires_at <= chrono::Utc::now().timestamp() {
            oauth::refresh(app, &tokens)?
        } else {
            tokens
        };
        Ok(Client {
            http: reqwest::blocking::Client::new(),
            access: tokens.access,
        })
    }

    fn get(&self, url: &str) -> reqwest::blocking::RequestBuilder {
        self.http.get(url).bearer_auth(&self.access)
    }
    fn post(&self, url: &str) -> reqwest::blocking::RequestBuilder {
        self.http.post(url).bearer_auth(&self.access)
    }

    /// Busy intervals on the primary calendar between two instants, in local time.
    pub fn free_busy(
        &self,
        min: DateTime<Local>,
        max: DateTime<Local>,
    ) -> Result<Vec<Busy>, String> {
        let body = serde_json::json!({
            "timeMin": min.to_rfc3339(),
            "timeMax": max.to_rfc3339(),
            "items": [{ "id": "primary" }],
        });
        let resp: serde_json::Value = self
            .post(&format!("{BASE}/freeBusy"))
            .json(&body)
            .send()
            .map_err(|err| format!("free/busy request failed: {err}"))?
            .error_for_status()
            .map_err(|err| format!("free/busy rejected: {err}"))?
            .json()
            .map_err(|err| format!("free/busy response unreadable: {err}"))?;

        let mut out = Vec::new();
        if let Some(busy) = resp
            .get("calendars")
            .and_then(|c| c.get("primary"))
            .and_then(|p| p.get("busy"))
            .and_then(|b| b.as_array())
        {
            for slot in busy {
                if let (Some(s), Some(e)) = (
                    slot.get("start").and_then(|v| v.as_str()).and_then(parse_local),
                    slot.get("end").and_then(|v| v.as_str()).and_then(parse_local),
                ) {
                    out.push((s, e));
                }
            }
        }
        Ok(out)
    }

    /// Create a 30-minute-ish event carrying the task id, so a later poll can
    /// tie a change in Google back to the task.
    pub fn insert_event(
        &self,
        task_id: i64,
        title: &str,
        start: DateTime<Local>,
        end: DateTime<Local>,
    ) -> Result<EventRef, String> {
        let body = serde_json::json!({
            "summary": title,
            "description": "Scheduled by Threshold.",
            "start": { "dateTime": start.to_rfc3339() },
            "end": { "dateTime": end.to_rfc3339() },
            "extendedProperties": { "private": { "thresholdTaskId": task_id.to_string() } },
            "reminders": { "useDefault": false, "overrides": [{ "method": "popup", "minutes": 10 }] },
        });
        let resp: serde_json::Value = self
            .post(&format!("{BASE}/calendars/primary/events"))
            .json(&body)
            .send()
            .map_err(|err| format!("could not create the event: {err}"))?
            .error_for_status()
            .map_err(|err| format!("Google refused the event: {err}"))?
            .json()
            .map_err(|err| format!("event response unreadable: {err}"))?;
        let id = resp
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or("Google returned an event with no id")?
            .to_string();
        let html_link = resp.get("htmlLink").and_then(|v| v.as_str()).map(String::from);
        Ok(EventRef { id, html_link })
    }

    /// Move an existing event to a new time.
    pub fn patch_event(
        &self,
        event_id: &str,
        start: DateTime<Local>,
        end: DateTime<Local>,
    ) -> Result<(), String> {
        let body = serde_json::json!({
            "start": { "dateTime": start.to_rfc3339() },
            "end": { "dateTime": end.to_rfc3339() },
        });
        self.http
            .patch(format!("{BASE}/calendars/primary/events/{event_id}"))
            .bearer_auth(&self.access)
            .json(&body)
            .send()
            .map_err(|err| format!("could not move the event: {err}"))?
            .error_for_status()
            .map_err(|err| format!("Google refused the move: {err}"))?;
        Ok(())
    }

    /// Delete an event. A 404/410 means it is already gone - success, not error.
    pub fn delete_event(&self, event_id: &str) -> Result<(), String> {
        let resp = self
            .http
            .delete(format!("{BASE}/calendars/primary/events/{event_id}"))
            .bearer_auth(&self.access)
            .send()
            .map_err(|err| format!("could not delete the event: {err}"))?;
        let status = resp.status().as_u16();
        if status == 404 || status == 410 || resp.status().is_success() {
            Ok(())
        } else {
            Err(format!("Google refused the delete: {}", resp.status()))
        }
    }

    /// The app's own events, and how they changed. Uses the sync token when we
    /// have one; a 410 GONE means the token expired - the caller drops it and
    /// re-lists from scratch. Returns `(items, next_sync_token)`.
    pub fn list_task_events(
        &self,
        sync_token: Option<&str>,
    ) -> Result<(Vec<EventItem>, Option<String>), String> {
        let mut url = format!("{BASE}/calendars/primary/events?singleEvents=true&showDeleted=true&maxResults=250");
        match sync_token {
            Some(tok) if !tok.is_empty() => {
                url.push_str("&syncToken=");
                url.push_str(&crate::popup::urlencode(tok));
            }
            _ => {
                // A first sync: only look back a day, forward is unbounded via
                // the token thereafter.
                let since = (Utc::now() - chrono::Duration::days(1)).to_rfc3339();
                url.push_str("&timeMin=");
                url.push_str(&crate::popup::urlencode(&since));
            }
        }

        let resp = self
            .get(&url)
            .send()
            .map_err(|err| format!("could not list events: {err}"))?;
        if resp.status().as_u16() == 410 {
            return Err("SYNC_TOKEN_EXPIRED".into());
        }
        let value: serde_json::Value = resp
            .error_for_status()
            .map_err(|err| format!("event list rejected: {err}"))?
            .json()
            .map_err(|err| format!("event list unreadable: {err}"))?;

        let mut items = Vec::new();
        if let Some(events) = value.get("items").and_then(|v| v.as_array()) {
            for ev in events {
                let Some(id) = ev.get("id").and_then(|v| v.as_str()) else { continue };
                let task_id = ev
                    .get("extendedProperties")
                    .and_then(|e| e.get("private"))
                    .and_then(|p| p.get("thresholdTaskId"))
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<i64>().ok());
                // Only ours.
                if task_id.is_none() {
                    continue;
                }
                let cancelled = ev.get("status").and_then(|v| v.as_str()) == Some("cancelled");
                let start_ts = ev
                    .get("start")
                    .and_then(|s| s.get("dateTime"))
                    .and_then(|v| v.as_str())
                    .and_then(parse_local)
                    .map(|dt| dt.timestamp());
                items.push(EventItem {
                    id: id.to_string(),
                    task_id,
                    cancelled,
                    start_ts,
                });
            }
        }
        let next = value
            .get("nextSyncToken")
            .and_then(|v| v.as_str())
            .map(String::from);
        Ok((items, next))
    }
}

/// Parse an RFC3339 timestamp (any offset) into local time.
fn parse_local(s: &str) -> Option<DateTime<Local>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| Local.from_utc_datetime(&dt.naive_utc()))
}

/// Read the stored sync token, if any.
pub fn sync_token(app: &AppHandle) -> Option<String> {
    let db = app.state::<Db>();
    let conn = db.0.lock().ok()?;
    db::get_setting(&conn, "google_sync_token").filter(|s| !s.is_empty())
}

pub fn save_sync_token(app: &AppHandle, token: Option<&str>) {
    let db = app.state::<Db>();
    let Ok(conn) = db.0.lock() else { return };
    let _ = db::set_setting(&conn, "google_sync_token", token.unwrap_or(""));
}
