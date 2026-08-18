//! Picking the next free slot for a scheduled task.
//!
//! Pure and local: given the clock, the user's working hours, how long the
//! task should take, and the busy blocks already on the calendar, this returns
//! the soonest start that fits. No network, no database - which is what makes
//! it testable, and what keeps the one piece of judgement in this whole feature
//! in a place a test can pin down.
//!
//! Everything is in the user's local time. A calendar is a human artefact -
//! "Tuesday at nine" means the wall clock in the room, not an instant in UTC -
//! so the arithmetic is done in `Local` and only converted to an offset string
//! at the API boundary. Working across a DST change therefore just works: the
//! local hours stay 09:00-18:00 and chrono carries the offset.

use chrono::{DateTime, Datelike, Duration, Local, TimeZone, Timelike};

/// When work happens, and how much air to leave around what is already booked.
#[derive(Debug, Clone, Copy)]
pub struct WorkingHours {
    /// Minutes past local midnight the day opens. 09:00 = 540.
    pub start_min: u32,
    /// Minutes past local midnight the day closes. 18:00 = 1080.
    pub end_min: u32,
    /// Which days count, Monday first. `[Mon, Tue, Wed, Thu, Fri, Sat, Sun]`.
    pub days: [bool; 7],
    /// Empty space to keep on either side of an existing event, so nothing
    /// lands back-to-back with a meeting.
    pub buffer_min: u32,
}

impl Default for WorkingHours {
    fn default() -> Self {
        Self {
            start_min: 9 * 60,
            end_min: 18 * 60,
            days: [true, true, true, true, true, false, false],
            buffer_min: 15,
        }
    }
}

impl WorkingHours {
    fn works_on(&self, date: DateTime<Local>) -> bool {
        self.days
            .get(date.weekday().num_days_from_monday() as usize)
            .copied()
            .unwrap_or(false)
    }
}

/// Round a time up to the next quarter hour, dropping seconds. A time already
/// exactly on a quarter is left where it is.
fn ceil_quarter(t: DateTime<Local>) -> DateTime<Local> {
    let whole = t
        .with_second(0)
        .and_then(|t| t.with_nanosecond(0))
        .unwrap_or(t);
    let rem = t.minute() % 15;
    if rem == 0 && t.second() == 0 && t.nanosecond() == 0 {
        return whole;
    }
    whole + Duration::minutes((15 - rem) as i64)
}

/// Local midnight starting the given day.
fn day_start(t: DateTime<Local>) -> DateTime<Local> {
    Local
        .with_ymd_and_hms(t.year(), t.month(), t.day(), 0, 0, 0)
        .single()
        .unwrap_or(t)
}

/// The soonest start, at or after `now`, that fits `duration` inside the
/// working hours and clears every busy block by `buffer_min` on both sides.
///
/// Searches up to 14 days out; `None` means the next fortnight is genuinely
/// full, which the caller reports rather than forcing a bad slot.
pub fn next_free_slot(
    now: DateTime<Local>,
    hours: &WorkingHours,
    duration: Duration,
    busy: &[(DateTime<Local>, DateTime<Local>)],
) -> Option<DateTime<Local>> {
    let buffer = Duration::minutes(hours.buffer_min as i64);
    let horizon = ceil_quarter(now) + Duration::days(14);
    let mut cursor = ceil_quarter(now);

    // The iteration cap only guarantees the loop ends; the real limit is the
    // horizon check below, so a calendar busy for a fortnight gives up rather
    // than scheduling a month out. 14 days of quarter-hours is a safe ceiling.
    for _ in 0..(14 * 24 * 4 + 8) {
        if cursor >= horizon {
            return None;
        }
        let midnight = day_start(cursor);
        let open = midnight + Duration::minutes(hours.start_min as i64);
        let close = midnight + Duration::minutes(hours.end_min as i64);

        // Not a working day, or already past closing: jump to the next day's open.
        if !hours.works_on(cursor) || cursor >= close {
            cursor =
                day_start(midnight + Duration::days(1)) + Duration::minutes(hours.start_min as i64);
            continue;
        }
        if cursor < open {
            cursor = open;
        }

        let end = cursor + duration;
        if end > close {
            // Won't fit before closing; try tomorrow.
            cursor =
                day_start(midnight + Duration::days(1)) + Duration::minutes(hours.start_min as i64);
            continue;
        }

        // The candidate must clear every busy block, buffered on both sides.
        // If it collides, step to the far edge of the latest offending block.
        let mut bumped_to: Option<DateTime<Local>> = None;
        for (bs, be) in busy {
            let blocked_start = *bs - buffer;
            let blocked_end = *be + buffer;
            if cursor < blocked_end && end > blocked_start {
                let after = ceil_quarter(blocked_end);
                bumped_to = Some(bumped_to.map_or(after, |b: DateTime<Local>| b.max(after)));
            }
        }
        match bumped_to {
            None => return Some(cursor),
            Some(next) => cursor = next,
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<Local> {
        Local
            .with_ymd_and_hms(y, mo, d, h, mi, 0)
            .single()
            .expect("valid local time")
    }

    fn hours() -> WorkingHours {
        WorkingHours::default()
    }

    const HALF: Duration = Duration::minutes(30);

    // 2024-06-03 is a Monday.
    #[test]
    fn inside_hours_and_free_takes_the_next_quarter() {
        let slot = next_free_slot(at(2024, 6, 3, 10, 7), &hours(), HALF, &[]);
        assert_eq!(slot, Some(at(2024, 6, 3, 10, 15)));
    }

    #[test]
    fn before_hours_waits_for_opening() {
        let slot = next_free_slot(at(2024, 6, 3, 7, 0), &hours(), HALF, &[]);
        assert_eq!(slot, Some(at(2024, 6, 3, 9, 0)));
    }

    #[test]
    fn after_hours_rolls_to_the_next_working_day() {
        let slot = next_free_slot(at(2024, 6, 3, 19, 0), &hours(), HALF, &[]);
        assert_eq!(slot, Some(at(2024, 6, 4, 9, 0)));
    }

    #[test]
    fn a_full_friday_evening_skips_the_weekend() {
        // Friday 2024-06-07 after close -> Monday 2024-06-10 09:00.
        let slot = next_free_slot(at(2024, 6, 7, 18, 30), &hours(), HALF, &[]);
        assert_eq!(slot, Some(at(2024, 6, 10, 9, 0)));
    }

    #[test]
    fn a_busy_block_pushes_past_it_with_buffer() {
        // Busy 10:00-10:30, buffer 15 -> a 10:15 start is blocked until 10:45.
        let busy = [(at(2024, 6, 3, 10, 0), at(2024, 6, 3, 10, 30))];
        let slot = next_free_slot(at(2024, 6, 3, 10, 7), &hours(), HALF, &busy);
        assert_eq!(slot, Some(at(2024, 6, 3, 10, 45)));
    }

    #[test]
    fn back_to_back_blocks_are_cleared_in_order() {
        let busy = [
            (at(2024, 6, 3, 9, 0), at(2024, 6, 3, 10, 0)),
            (at(2024, 6, 3, 10, 15), at(2024, 6, 3, 11, 0)),
        ];
        // 09:00 start blocked to 10:15; that start blocked to 11:15.
        let slot = next_free_slot(at(2024, 6, 3, 9, 0), &hours(), HALF, &busy);
        assert_eq!(slot, Some(at(2024, 6, 3, 11, 15)));
    }

    #[test]
    fn a_gap_too_small_for_the_buffers_is_rejected() {
        let busy = [
            (at(2024, 6, 3, 9, 0), at(2024, 6, 3, 10, 0)),
            (at(2024, 6, 3, 10, 45), at(2024, 6, 3, 12, 0)),
        ];
        // 10:15 (after first buffer) + 30 = 10:45, which overlaps the second
        // block's 10:30 buffered start -> pushed to 12:15.
        let slot = next_free_slot(at(2024, 6, 3, 9, 0), &hours(), HALF, &busy);
        assert_eq!(slot, Some(at(2024, 6, 3, 12, 15)));
    }

    #[test]
    fn zero_buffer_allows_touching_a_block() {
        let mut h = hours();
        h.buffer_min = 0;
        let busy = [(at(2024, 6, 3, 9, 0), at(2024, 6, 3, 10, 0))];
        let slot = next_free_slot(at(2024, 6, 3, 9, 0), &h, HALF, &busy);
        assert_eq!(slot, Some(at(2024, 6, 3, 10, 0)));
    }

    #[test]
    fn a_fortnight_of_solid_busy_gives_up() {
        let busy = [(at(2024, 6, 3, 0, 0), at(2024, 6, 30, 0, 0))];
        let slot = next_free_slot(at(2024, 6, 3, 9, 0), &hours(), HALF, &busy);
        assert_eq!(slot, None);
    }
}
