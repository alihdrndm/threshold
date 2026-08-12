//! The quote reservoir.
//!
//! Lines the user keeps, shown at the two moments an urge actually arrives: the
//! ritual, and a site that refuses to load.
//!
//! Which quote appears where is a setting, not a property of the quote, so the
//! same line can serve both surfaces and neither has to own it. `shuffle` is the
//! default because a fixed line is habituated exactly like a fixed dialog is —
//! the same reason the ritual rotates its phrasing daily.

use rusqlite::Connection;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Quote {
    pub id: i64,
    pub text: String,
    pub author: Option<String>,
}

/// A quote may be long, but not a document. Past this it stops being something
/// you take in at a glance, which is the only job it has here.
pub const MAX_LEN: usize = 280;

/// Where a quote is being shown. Two surfaces, chosen independently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    Ritual,
    Blocked,
}

impl Surface {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "ritual" => Some(Surface::Ritual),
            "blocked" => Some(Surface::Blocked),
            _ => None,
        }
    }

    /// The settings key holding this surface's choice: `shuffle`, or an id.
    pub fn setting(self) -> &'static str {
        match self {
            Surface::Ritual => "quote_ritual",
            Surface::Blocked => "quote_blocked",
        }
    }
}

/// Tidy a typed quote, or say why it cannot be kept.
pub fn clean(text: &str) -> Result<String, String> {
    // Collapse the newlines a paste brings with it: these are rendered in one
    // block, and a pasted line break would show up as a gap nobody intended.
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        return Err("that is empty".into());
    }
    if text.chars().count() > MAX_LEN {
        return Err(format!(
            "that is {} characters; the limit is {MAX_LEN}",
            text.chars().count()
        ));
    }
    Ok(text)
}

pub fn add(conn: &Connection, text: &str, author: Option<&str>) -> Result<i64, String> {
    let text = clean(text)?;
    let author = author
        .map(str::trim)
        .filter(|author| !author.is_empty())
        .map(str::to_owned);

    conn.execute(
        "INSERT INTO quotes(text, author, created_ts) VALUES(?1, ?2, datetime('now'))
         ON CONFLICT(text) DO UPDATE SET author = excluded.author",
        rusqlite::params![text, author],
    )
    .map_err(|err| format!("could not save the quote: {err}"))?;

    // The upsert means a repeat returns the row that already existed rather
    // than a new one, so adding a duplicate quietly updates it instead of
    // failing at the user.
    conn.query_row("SELECT id FROM quotes WHERE text = ?1", [&text], |row| {
        row.get(0)
    })
    .map_err(|err| format!("could not save the quote: {err}"))
}

pub fn all(conn: &Connection) -> Result<Vec<Quote>, String> {
    let mut stmt = conn
        .prepare("SELECT id, text, author FROM quotes ORDER BY id")
        .map_err(|err| format!("could not read quotes: {err}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok(Quote {
                id: row.get(0)?,
                text: row.get(1)?,
                author: row.get(2)?,
            })
        })
        .map_err(|err| format!("could not read quotes: {err}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("could not read quotes: {err}"))
}

pub fn remove(conn: &Connection, id: i64) -> Result<(), String> {
    conn.execute("DELETE FROM quotes WHERE id = ?1", [id])
        .map(|_| ())
        .map_err(|err| format!("could not remove the quote: {err}"))
}

/// The quote to show on a surface right now.
///
/// `None` is a first-class answer: with nothing in the reservoir the surface
/// shows nothing at all, rather than a stand-in nobody chose. A quote the app
/// picked for you is exactly the kind of borrowed sentiment this feature exists
/// to replace.
pub fn for_surface(conn: &Connection, surface: Surface) -> Result<Option<Quote>, String> {
    let choice = super::get_setting(conn, surface.setting()).unwrap_or_default();

    if let Ok(id) = choice.parse::<i64>() {
        let pinned = conn
            .query_row(
                "SELECT id, text, author FROM quotes WHERE id = ?1",
                [id],
                |row| {
                    Ok(Quote {
                        id: row.get(0)?,
                        text: row.get(1)?,
                        author: row.get(2)?,
                    })
                },
            )
            .ok();
        // A pinned quote that has since been deleted falls back to shuffling
        // rather than showing nothing: the setting is stale, the reservoir is
        // not.
        if pinned.is_some() {
            return Ok(pinned);
        }
    }

    // Shuffle. RANDOM() rather than a stored cursor because the point is that
    // you cannot predict it — a rotation you can anticipate is habituated just
    // as fast as a fixed line.
    conn.query_row(
        "SELECT id, text, author FROM quotes ORDER BY RANDOM() LIMIT 1",
        [],
        |row| {
            Ok(Quote {
                id: row.get(0)?,
                text: row.get(1)?,
                author: row.get(2)?,
            })
        },
    )
    .map(Some)
    .or_else(|err| match err {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(format!("could not pick a quote: {other}")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().expect("memory db");
        crate::db::migrate_for_tests(&conn);
        conn
    }

    #[test]
    fn a_quote_survives_a_round_trip() {
        let conn = db();
        let id = add(&conn, "The obstacle is the way", Some("Aurelius")).expect("add");
        let all = all(&conn).expect("all");
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, id);
        assert_eq!(all[0].author.as_deref(), Some("Aurelius"));
    }

    #[test]
    fn an_author_is_optional() {
        let conn = db();
        add(&conn, "Begin", None).expect("add");
        assert_eq!(all(&conn).expect("all")[0].author, None);
    }

    #[test]
    fn a_pasted_line_break_does_not_become_a_gap() {
        assert_eq!(clean("  two\n\n  words  "), Ok("two words".to_string()));
    }

    #[test]
    fn an_empty_or_overlong_quote_is_refused() {
        assert!(clean("   ").is_err());
        assert!(clean(&"a".repeat(MAX_LEN + 1)).is_err());
        assert!(clean(&"a".repeat(MAX_LEN)).is_ok());
    }

    #[test]
    fn adding_the_same_words_twice_does_not_make_two_quotes() {
        let conn = db();
        let first = add(&conn, "Begin", None).expect("add");
        let again = add(&conn, "Begin", Some("me")).expect("add again");
        assert_eq!(first, again);
        assert_eq!(all(&conn).expect("all").len(), 1);
        assert_eq!(all(&conn).expect("all")[0].author.as_deref(), Some("me"));
    }

    #[test]
    fn an_empty_reservoir_shows_nothing_rather_than_a_stand_in() {
        let conn = db();
        assert!(for_surface(&conn, Surface::Ritual).expect("pick").is_none());
    }

    #[test]
    fn a_pinned_quote_is_the_one_that_shows() {
        let conn = db();
        add(&conn, "one", None).expect("add");
        let pinned = add(&conn, "two", None).expect("add");
        crate::db::set_setting(&conn, Surface::Ritual.setting(), &pinned.to_string())
            .expect("pin");

        for _ in 0..8 {
            let got = for_surface(&conn, Surface::Ritual).expect("pick").unwrap();
            assert_eq!(got.id, pinned);
        }
    }

    /// A stale pin must not blank the surface — the setting is out of date, the
    /// reservoir is fine.
    #[test]
    fn a_pin_pointing_at_a_deleted_quote_falls_back_to_shuffling() {
        let conn = db();
        let doomed = add(&conn, "gone", None).expect("add");
        add(&conn, "still here", None).expect("add");
        crate::db::set_setting(&conn, Surface::Ritual.setting(), &doomed.to_string())
            .expect("pin");
        remove(&conn, doomed).expect("remove");

        let got = for_surface(&conn, Surface::Ritual).expect("pick");
        assert_eq!(got.map(|quote| quote.text), Some("still here".to_string()));
    }

    #[test]
    fn the_two_surfaces_are_chosen_independently() {
        let conn = db();
        let a = add(&conn, "for the ritual", None).expect("add");
        let b = add(&conn, "for the wall", None).expect("add");
        crate::db::set_setting(&conn, Surface::Ritual.setting(), &a.to_string()).expect("pin");
        crate::db::set_setting(&conn, Surface::Blocked.setting(), &b.to_string()).expect("pin");

        assert_eq!(
            for_surface(&conn, Surface::Ritual).expect("pick").unwrap().id,
            a
        );
        assert_eq!(
            for_surface(&conn, Surface::Blocked).expect("pick").unwrap().id,
            b
        );
    }

    #[test]
    fn shuffling_reaches_every_quote() {
        let conn = db();
        for text in ["a", "b", "c"] {
            add(&conn, text, None).expect("add");
        }
        let mut seen = std::collections::HashSet::new();
        for _ in 0..200 {
            seen.insert(for_surface(&conn, Surface::Ritual).expect("pick").unwrap().id);
        }
        assert_eq!(seen.len(), 3, "shuffle must not strand a quote");
    }
}
