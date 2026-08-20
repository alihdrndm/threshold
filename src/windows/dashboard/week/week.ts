/**
 * The geometry of the week panel: intervals in, percentages out.
 *
 * Pure and local, like slot.rs on the other side: a calendar is a human
 * artefact, so every position is derived from the local wall clock
 * (getHours/getMinutes), never from elapsed seconds since midnight. On a DST
 * day of 23 or 25 hours, a 14:00 meeting still draws at 14:00 - which is
 * where the user's own calendar draws it.
 *
 * The one subtlety this module owns: Threshold's scheduled tasks appear twice
 * in the data - once as local tasks, once inside Google's opaque busy
 * intervals (Google does not say which is which). The overlay IS the dedupe:
 * task blocks render after busy blocks, covering their own busy footprint.
 */

export interface Interval {
  /** Unix seconds. */
  start: number;
  end: number;
}

/** The vertical span the grid shows, minutes past local midnight. */
export interface Axis {
  startMin: number;
  endMin: number;
}

export interface Block {
  kind: "busy" | "task";
  /** Clipped to the day, unix seconds. */
  start: number;
  end: number;
  topPct: number;
  heightPct: number;
  taskId?: number;
  title?: string;
}

export interface DayColumn {
  /** Local midnight, unix seconds. */
  dayStart: number;
  /** 0 = Monday, matching the backend's Monday-first `days`. */
  weekdayIndex: number;
  /** Whether this is a working day under the user's hours. */
  working: boolean;
  /** Busy first, then task overlays, in render order. */
  blocks: Block[];
  /** Where the now-line sits, or null unless this is today and on-axis. */
  nowPct: number | null;
}

/** Minutes past local midnight of an instant, by the wall clock. */
export function minuteOf(ts: number): number {
  const d = new Date(ts * 1000);
  return d.getHours() * 60 + d.getMinutes();
}

/** JS Sunday-first weekday to the app's Monday-first index. */
export function mondayIndex(ts: number): number {
  return (new Date(ts * 1000).getDay() + 6) % 7;
}

/**
 * The local midnights of `count` consecutive days from `startTs` (itself a
 * local midnight). Stepped with setDate, never +86 400 - a DST day is 23 or
 * 25 hours long and fixed spacing would drift every column after it.
 */
export function weekDays(startTs: number, count = 7): number[] {
  const days: number[] = [];
  const d = new Date(startTs * 1000);
  d.setHours(0, 0, 0, 0);
  for (let i = 0; i < count; i += 1) {
    days.push(Math.floor(d.getTime() / 1000));
    d.setDate(d.getDate() + 1);
    d.setHours(0, 0, 0, 0);
  }
  return days;
}

/**
 * The span the grid displays: the working hours with two hours of air either
 * side, clamped to the day - enough to show an early or late booking without
 * drawing twenty empty rows of night. Degenerate hours (end at or before
 * start) fall back to the whole day rather than an inside-out axis.
 */
export function axisRange(hours: { startMin: number; endMin: number }): Axis {
  if (hours.endMin <= hours.startMin) return { startMin: 0, endMin: 1440 };
  return {
    startMin: Math.max(0, hours.startMin - 120),
    endMin: Math.min(1440, hours.endMin + 120),
  };
}

/** Sort and merge overlapping or touching intervals into a quiet minimum. */
export function mergeIntervals(intervals: Interval[]): Interval[] {
  const sorted = [...intervals].sort((a, b) => a.start - b.start);
  const merged: Interval[] = [];
  for (const iv of sorted) {
    const last = merged[merged.length - 1];
    if (last && iv.start <= last.end) {
      last.end = Math.max(last.end, iv.end);
    } else {
      merged.push({ ...iv });
    }
  }
  return merged;
}

/**
 * The part of an interval that falls on one day, or null when none does. A
 * block spanning midnight therefore appears split across two columns - the
 * way every calendar draws it.
 */
export function clipToDay(
  iv: Interval,
  dayStart: number,
  nextDayStart: number,
): Interval | null {
  const start = Math.max(iv.start, dayStart);
  const end = Math.min(iv.end, nextDayStart);
  return start < end ? { start, end } : null;
}

/**
 * Where a (day-clipped) interval sits on the axis, as percentages. Partly
 * off-axis clamps to the edge - a 6 a.m. flight on a 7 a.m. axis must still
 * show something rather than silently hide a conflict; only a block entirely
 * outside the axis returns null.
 */
export function place(
  iv: Interval,
  axis: Axis,
): { topPct: number; heightPct: number } | null {
  const span = axis.endMin - axis.startMin;
  if (span <= 0) return null;
  // End-of-day clip lands at the NEXT midnight, whose wall clock reads 0:00;
  // treat it as minute 1440 of this day so the block reaches the bottom.
  const rawEnd = minuteOf(iv.end);
  const endMin = rawEnd === 0 && iv.end > iv.start ? 1440 : rawEnd;
  const top = Math.max(minuteOf(iv.start), axis.startMin);
  const bottom = Math.min(endMin, axis.endMin);
  if (bottom <= top) return null;
  return {
    topPct: ((top - axis.startMin) / span) * 100,
    heightPct: ((bottom - top) / span) * 100,
  };
}

/** The tick marks for the hour axis, minutes past midnight. */
export function hourTicks(axis: Axis, stepMin = 60): number[] {
  const ticks: number[] = [];
  const first = Math.ceil(axis.startMin / stepMin) * stepMin;
  for (let m = first; m <= axis.endMin; m += stepMin) {
    ticks.push(m);
  }
  return ticks;
}

/** A tick as the user's clock writes it ("9 AM" or "09:00" per locale). */
export function tickLabel(minute: number, day: Date = new Date()): string {
  const d = new Date(day);
  d.setHours(Math.floor(minute / 60), minute % 60, 0, 0);
  return d.toLocaleTimeString(undefined, { hour: "numeric" });
}

/** The 30-minute slot every Threshold event occupies (sync.rs's SLOT). */
export const TASK_SLOT_SECS = 30 * 60;

/**
 * The whole computation: the axis and seven columns of positioned blocks.
 * `tasks` should already be filtered to open tasks with a scheduled time;
 * each becomes a titled "task" block over `[scheduledTs, +30 min)`, rendered
 * after the busy blocks so it covers its own footprint in Google's data.
 */
export function buildWeek(
  data: {
    startTs: number;
    busy: Interval[];
    hours: { startMin: number; endMin: number; days: boolean[] };
  },
  tasks: { id: number; title: string; scheduledTs: number }[],
  now: number,
): { axis: Axis; days: DayColumn[] } {
  const axis = axisRange(data.hours);
  const starts = weekDays(data.startTs);
  const busy = mergeIntervals(data.busy);

  const days = starts.map((dayStart, i) => {
    const nextDayStart =
      i + 1 < starts.length ? starts[i + 1] : weekDays(dayStart, 2)[1];
    const weekdayIndex = mondayIndex(dayStart);

    const blocks: Block[] = [];
    for (const iv of busy) {
      const clipped = clipToDay(iv, dayStart, nextDayStart);
      if (!clipped) continue;
      const pos = place(clipped, axis);
      if (!pos) continue;
      blocks.push({ kind: "busy", ...clipped, ...pos });
    }
    for (const task of tasks) {
      const clipped = clipToDay(
        { start: task.scheduledTs, end: task.scheduledTs + TASK_SLOT_SECS },
        dayStart,
        nextDayStart,
      );
      if (!clipped) continue;
      const pos = place(clipped, axis);
      if (!pos) continue;
      blocks.push({
        kind: "task",
        ...clipped,
        ...pos,
        taskId: task.id,
        title: task.title,
      });
    }

    const isToday = now >= dayStart && now < nextDayStart;
    const nowMin = minuteOf(now);
    const nowPct =
      isToday && nowMin >= axis.startMin && nowMin <= axis.endMin
        ? ((nowMin - axis.startMin) / (axis.endMin - axis.startMin)) * 100
        : null;

    return {
      dayStart,
      weekdayIndex,
      working: data.hours.days[weekdayIndex] ?? false,
      blocks,
      nowPct,
    };
  });

  return { axis, days };
}
