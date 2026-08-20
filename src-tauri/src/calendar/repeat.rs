//! When a repeating task comes back: the next matching day, same wall-clock
//! time.
//!
//! Pure and local, like `slot` - the one piece of judgement in the repeat
//! feature lives where a test can pin it down. No busy blocks and no slot
//! hunting here on purpose: a repeating task is a ritual, and a ritual keeps
//! its time. Conflicts stay visible on the week panel, where the person - not
//! an algorithm - decides which one moves.

use chrono::{DateTime, Datelike, Duration, Local, LocalResult, TimeZone};

/// Tidy a "5,3,3,1" mask into "1,3,5": each piece a digit 1-7 (Mon=1..Sun=7),
/// deduped, ascending. Anything else - or an empty result - is an error: a
/// repeat with no days is "off", and off is spelled None, not "".
pub fn normalize_days(raw: &str) -> Result<String, String> {
    let mut days = [false; 7];
    for piece in raw.split(',') {
        let piece = piece.trim();
        if piece.is_empty() {
            continue;
        }
        match piece.parse::<usize>() {
            Ok(n) if (1..=7).contains(&n) => days[n - 1] = true,
            _ => return Err(format!("not a weekday (1-7): {piece}")),
        }
    }
    let listed: Vec<String> = days
        .iter()
        .enumerate()
        .filter(|(_, on)| **on)
        .map(|(i, _)| (i + 1).to_string())
        .collect();
    if listed.is_empty() {
        return Err("a repeat needs at least one day".into());
    }
    Ok(listed.join(","))
}

/// "1,3,5" -> the Monday-first `[bool; 7]` the rest of the calendar speaks.
pub fn day_mask(raw: &str) -> [bool; 7] {
    super::settings::parse_days(Some(raw.to_string()))
}

/// The next day STRICTLY after `after`'s date whose weekday is in `days`, at
/// `hour:minute` local wall-clock. Same-day never matches: completing
/// Monday's task with Monday in the mask goes to the NEXT matching day, which
/// may be next Monday. `None` when the mask is empty.
///
/// DST-safe the way a wall clock is: a fall-back ambiguity takes the earlier
/// offset, and a spring-forward gap (02:30 does not exist that day) slides an
/// hour later rather than vanishing.
pub fn next_occurrence(
    after: DateTime<Local>,
    days: [bool; 7],
    hour: u32,
    minute: u32,
) -> Option<DateTime<Local>> {
    if !days.iter().any(|d| *d) {
        return None;
    }
    for step in 1..=7 {
        let date = after.date_naive() + Duration::days(step);
        if days[date.weekday().num_days_from_monday() as usize] {
            return match Local.with_ymd_and_hms(date.year(), date.month(), date.day(), hour, minute, 0)
            {
                LocalResult::Single(t) => Some(t),
                LocalResult::Ambiguous(early, _) => Some(early),
                LocalResult::None => Local
                    .with_ymd_and_hms(date.year(), date.month(), date.day(), hour + 1, minute, 0)
                    .single(),
            };
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;

    fn at(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(y, mo, d, h, mi, 0).single().unwrap()
    }

    const DAILY: [bool; 7] = [true; 7];

    #[test]
    fn a_daily_task_comes_back_tomorrow() {
        // 2026-06-01 is a Monday.
        let next = next_occurrence(at(2026, 6, 1, 9, 0), DAILY, 9, 0).unwrap();
        assert_eq!(next, at(2026, 6, 2, 9, 0));
    }

    #[test]
    fn the_same_day_never_matches() {
        // Mon+Wed mask, completed on Monday: Wednesday, not this Monday again.
        let days = day_mask("1,3");
        let next = next_occurrence(at(2026, 6, 1, 9, 0), days, 9, 0).unwrap();
        assert_eq!(next, at(2026, 6, 3, 9, 0));
    }

    #[test]
    fn a_single_day_mask_waits_a_full_week() {
        let days = day_mask("1");
        let next = next_occurrence(at(2026, 6, 1, 9, 0), days, 9, 0).unwrap();
        assert_eq!(next, at(2026, 6, 8, 9, 0));
    }

    #[test]
    fn the_search_wraps_past_sunday() {
        // Tuesday mask, completed on Saturday 2026-06-06.
        let days = day_mask("2");
        let next = next_occurrence(at(2026, 6, 6, 14, 0), days, 14, 0).unwrap();
        assert_eq!(next, at(2026, 6, 9, 14, 0));
    }

    #[test]
    fn an_empty_mask_is_no_repeat() {
        assert!(next_occurrence(at(2026, 6, 1, 9, 0), [false; 7], 9, 0).is_none());
    }

    #[test]
    fn minutes_survive_the_advance() {
        let next = next_occurrence(at(2026, 6, 1, 9, 30), DAILY, 9, 30).unwrap();
        assert_eq!((next.hour(), next.minute()), (9, 30));
    }

    #[test]
    fn a_dst_week_keeps_the_wall_clock_time() {
        // US spring-forward is Sunday 2026-03-08. Friday before, weekday mask:
        // the next occurrence is Monday 09:00 by the wall clock, whatever the
        // zone did to the offset in between. (In zones without DST this is
        // simply a plain Monday - the assertion holds either way.)
        let days = day_mask("1,2,3,4,5");
        let next = next_occurrence(at(2026, 3, 6, 9, 0), days, 9, 0).unwrap();
        assert_eq!(next, at(2026, 3, 9, 9, 0));
        assert_eq!((next.hour(), next.minute()), (9, 0));
    }

    #[test]
    fn normalize_tidies_and_refuses() {
        assert_eq!(normalize_days("5,3,3,1").unwrap(), "1,3,5");
        assert_eq!(normalize_days(" 2 ").unwrap(), "2");
        assert_eq!(normalize_days("1,2,3,4,5,6,7").unwrap(), "1,2,3,4,5,6,7");
        assert!(normalize_days("0").is_err());
        assert!(normalize_days("8").is_err());
        assert!(normalize_days("mon").is_err());
        assert!(normalize_days("").is_err());
    }
}
