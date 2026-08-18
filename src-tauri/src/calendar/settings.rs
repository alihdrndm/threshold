//! The working-hours settings, read into the shape the slot picker wants.
//!
//! Stored as plain strings in the same key/value table as everything else, and
//! read with defaults so a fresh install schedules sensibly before the user
//! has opened Settings at all.

use rusqlite::Connection;

use super::slot::WorkingHours;
use crate::db;

/// "09:00" -> minutes past midnight. Anything malformed falls back.
fn parse_hhmm(raw: Option<String>, default_min: u32) -> u32 {
    raw.and_then(|s| {
        let (h, m) = s.trim().split_once(':')?;
        let h: u32 = h.trim().parse().ok()?;
        let m: u32 = m.trim().parse().ok()?;
        (h < 24 && m < 60).then_some(h * 60 + m)
    })
    .unwrap_or(default_min)
}

/// "1,2,3,4,5" (Mon=1 .. Sun=7) -> a Monday-first mask.
fn parse_days(raw: Option<String>) -> [bool; 7] {
    match raw {
        None => [true, true, true, true, true, false, false],
        Some(s) => {
            let mut days = [false; 7];
            for piece in s.split(',') {
                if let Ok(n) = piece.trim().parse::<usize>() {
                    if (1..=7).contains(&n) {
                        days[n - 1] = true;
                    }
                }
            }
            days
        }
    }
}

pub fn read_hours(conn: &Connection) -> WorkingHours {
    WorkingHours {
        start_min: parse_hhmm(db::get_setting(conn, "work_start"), 9 * 60),
        end_min: parse_hhmm(db::get_setting(conn, "work_end"), 18 * 60),
        days: parse_days(db::get_setting(conn, "work_days")),
        buffer_min: db::get_setting(conn, "cal_buffer_min")
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(15),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_a_weekday_nine_to_six() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrate_for_tests(&conn);
        let h = read_hours(&conn);
        assert_eq!((h.start_min, h.end_min, h.buffer_min), (540, 1080, 15));
        assert_eq!(h.days, [true, true, true, true, true, false, false]);
    }

    #[test]
    fn stored_values_are_read_and_bad_ones_fall_back() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrate_for_tests(&conn);
        db::set_setting(&conn, "work_start", "08:30").unwrap();
        db::set_setting(&conn, "work_end", "not-a-time").unwrap();
        db::set_setting(&conn, "work_days", "6,7").unwrap();
        db::set_setting(&conn, "cal_buffer_min", "5").unwrap();
        let h = read_hours(&conn);
        assert_eq!(h.start_min, 8 * 60 + 30);
        assert_eq!(h.end_min, 18 * 60, "malformed close falls back to default");
        assert_eq!(h.days, [false, false, false, false, false, true, true]);
        assert_eq!(h.buffer_min, 5);
    }
}
