import { describe, expect, it } from "vitest";
import {
  axisRange,
  buildDay,
  buildWeek,
  clipToDay,
  echoOn,
  formatDuration,
  freeGaps,
  hourTicks,
  mergeIntervals,
  mondayIndex,
  place,
  weekDays,
} from "./week";

/** Unix seconds for a local date-time, the way the fixtures read. */
const at = (y: number, mo: number, d: number, h = 0, mi = 0) =>
  Math.floor(new Date(y, mo - 1, d, h, mi).getTime() / 1000);

// A Monday.
const MONDAY = at(2026, 8, 24);
const HOURS = { startMin: 9 * 60, endMin: 18 * 60, days: [true, true, true, true, true, false, false] };

describe("mergeIntervals", () => {
  it("sorts and merges overlapping intervals", () => {
    const merged = mergeIntervals([
      { start: 100, end: 200 },
      { start: 50, end: 120 },
    ]);
    expect(merged).toEqual([{ start: 50, end: 200 }]);
  });

  it("merges intervals that exactly touch", () => {
    expect(
      mergeIntervals([
        { start: 0, end: 100 },
        { start: 100, end: 200 },
      ]),
    ).toEqual([{ start: 0, end: 200 }]);
  });

  it("keeps disjoint intervals apart and does not mutate its input", () => {
    const input = [
      { start: 300, end: 400 },
      { start: 0, end: 100 },
    ];
    const merged = mergeIntervals(input);
    expect(merged).toEqual([
      { start: 0, end: 100 },
      { start: 300, end: 400 },
    ]);
    expect(input[0]).toEqual({ start: 300, end: 400 });
  });
});

describe("clipToDay", () => {
  const day = MONDAY;
  const next = at(2026, 8, 25);

  it("splits a midnight-spanning interval across two columns", () => {
    const iv = { start: at(2026, 8, 24, 23), end: at(2026, 8, 25, 1) };
    expect(clipToDay(iv, day, next)).toEqual({
      start: at(2026, 8, 24, 23),
      end: next,
    });
    expect(clipToDay(iv, next, at(2026, 8, 26))).toEqual({
      start: next,
      end: at(2026, 8, 25, 1),
    });
  });

  it("returns null when the interval misses the day entirely", () => {
    expect(
      clipToDay({ start: at(2026, 8, 26, 9), end: at(2026, 8, 26, 10) }, day, next),
    ).toBeNull();
  });

  it("covers full middle days of a multi-day interval", () => {
    const iv = { start: at(2026, 8, 23, 12), end: at(2026, 8, 26, 12) };
    expect(clipToDay(iv, day, next)).toEqual({ start: day, end: next });
  });
});

describe("place", () => {
  // 07:00-20:00 axis: the default hours with two hours of air.
  const axis = axisRange(HOURS);

  it("gives the default hours two hours of air either side", () => {
    expect(axis).toEqual({ startMin: 420, endMin: 1200 });
  });

  it("positions a block by wall-clock minutes", () => {
    const pos = place(
      { start: at(2026, 8, 24, 9), end: at(2026, 8, 24, 9, 30) },
      axis,
    );
    expect(pos?.topPct).toBeCloseTo(((540 - 420) / 780) * 100);
    expect(pos?.heightPct).toBeCloseTo((30 / 780) * 100);
  });

  it("clamps a partly off-axis block to the edge instead of hiding it", () => {
    // A 6 a.m. flight on a 7 a.m. axis: starts at the very top.
    const pos = place(
      { start: at(2026, 8, 24, 6), end: at(2026, 8, 24, 8) },
      axis,
    );
    expect(pos?.topPct).toBe(0);
    expect(pos?.heightPct).toBeCloseTo((60 / 780) * 100);
  });

  it("drops a block entirely outside the axis", () => {
    expect(
      place({ start: at(2026, 8, 24, 2), end: at(2026, 8, 24, 3) }, axis),
    ).toBeNull();
  });

  it("lets a day-end clip reach the bottom of a full-day axis", () => {
    // Clipped at next midnight, whose wall clock reads 0:00 - must not vanish.
    const fullDay = { startMin: 0, endMin: 1440 };
    const pos = place(
      { start: at(2026, 8, 24, 23), end: at(2026, 8, 25) },
      fullDay,
    );
    expect(pos?.heightPct).toBeCloseTo((60 / 1440) * 100);
    expect(pos!.topPct + pos!.heightPct).toBeCloseTo(100);
  });
});

describe("axisRange", () => {
  it("clamps to the day at both ends", () => {
    expect(axisRange({ startMin: 60, endMin: 1400 })).toEqual({
      startMin: 0,
      endMin: 1440,
    });
  });

  it("falls back to the whole day when hours are degenerate", () => {
    expect(axisRange({ startMin: 600, endMin: 600 })).toEqual({
      startMin: 0,
      endMin: 1440,
    });
  });
});

describe("weekDays", () => {
  it("yields seven true local midnights", () => {
    const days = weekDays(MONDAY);
    expect(days).toHaveLength(7);
    for (const ts of days) {
      const d = new Date(ts * 1000);
      expect(d.getHours()).toBe(0);
      expect(d.getMinutes()).toBe(0);
    }
  });

  it("steps by date, not by fixed 86 400 seconds", () => {
    // Across the US DST fall-back week (Nov 1 2026) one gap is 25 hours in
    // zones that observe it; either way each entry must be a local midnight
    // and the dates must be consecutive.
    const days = weekDays(at(2026, 10, 31));
    expect(days).toHaveLength(7);
    let previous: Date | null = null;
    for (const ts of days) {
      const d = new Date(ts * 1000);
      expect(d.getHours()).toBe(0);
      if (previous) {
        const step = new Date(previous);
        step.setDate(step.getDate() + 1);
        expect(d.getDate()).toBe(step.getDate());
        expect(d.getMonth()).toBe(step.getMonth());
      }
      previous = d;
    }
  });
});

describe("mondayIndex", () => {
  it("maps JS Sunday-first weekdays to Monday-first", () => {
    expect(mondayIndex(MONDAY)).toBe(0); // a Monday
    expect(mondayIndex(at(2026, 8, 29))).toBe(5); // a Saturday
    expect(mondayIndex(at(2026, 8, 30))).toBe(6); // a Sunday
  });
});

describe("hourTicks", () => {
  it("marks whole hours inside the axis", () => {
    const ticks = hourTicks({ startMin: 420, endMin: 600 });
    expect(ticks).toEqual([420, 480, 540, 600]);
  });

  it("respects a coarser step", () => {
    expect(hourTicks({ startMin: 420, endMin: 720 }, 120)).toEqual([480, 600, 720]);
  });
});

describe("freeGaps", () => {
  const before = at(2026, 8, 23, 12); // Sunday noon, so Monday is untouched by "now"

  it("finds the gaps between busy blocks inside working hours", () => {
    const gaps = freeGaps(
      [
        { start: at(2026, 8, 24, 10), end: at(2026, 8, 24, 11) },
        { start: at(2026, 8, 24, 14), end: at(2026, 8, 24, 16) },
      ],
      MONDAY,
      HOURS,
      before,
    );
    expect(gaps).toEqual([
      { start: at(2026, 8, 24, 9), end: at(2026, 8, 24, 10) },
      { start: at(2026, 8, 24, 11), end: at(2026, 8, 24, 14) },
      { start: at(2026, 8, 24, 16), end: at(2026, 8, 24, 18) },
    ]);
  });

  it("gives an empty day one gap spanning the whole working window", () => {
    expect(freeGaps([], MONDAY, HOURS, before)).toEqual([
      { start: at(2026, 8, 24, 9), end: at(2026, 8, 24, 18) },
    ]);
  });

  it("starts today's gaps at now, not at the day's open", () => {
    const gaps = freeGaps([], MONDAY, HOURS, at(2026, 8, 24, 13, 30));
    expect(gaps).toEqual([
      { start: at(2026, 8, 24, 13, 30), end: at(2026, 8, 24, 18) },
    ]);
  });

  it("returns nothing on a non-working day or a spent one", () => {
    const saturday = at(2026, 8, 29);
    expect(freeGaps([], saturday, HOURS, before)).toEqual([]);
    expect(freeGaps([], MONDAY, HOURS, at(2026, 8, 24, 19))).toEqual([]);
  });

  it("drops slivers under fifteen minutes", () => {
    const gaps = freeGaps(
      [{ start: at(2026, 8, 24, 9, 10), end: at(2026, 8, 24, 18) }],
      MONDAY,
      HOURS,
      before,
    );
    expect(gaps).toEqual([]); // the 9:00-9:10 sliver is noise
  });

  it("ignores busy outside the working window", () => {
    const gaps = freeGaps(
      [{ start: at(2026, 8, 24, 6), end: at(2026, 8, 24, 7) }],
      MONDAY,
      HOURS,
      before,
    );
    expect(gaps).toEqual([
      { start: at(2026, 8, 24, 9), end: at(2026, 8, 24, 18) },
    ]);
  });
});

describe("formatDuration", () => {
  it("speaks minutes, hours, and both", () => {
    expect(formatDuration(45 * 60)).toBe("45 min");
    expect(formatDuration(2 * 3600)).toBe("2 h");
    expect(formatDuration(90 * 60)).toBe("1 h 30 min");
  });
});

describe("buildDay", () => {
  const data = {
    busy: [{ start: at(2026, 8, 24, 10), end: at(2026, 8, 24, 11) }],
    hours: HOURS,
  };

  it("uses the full day as its axis", () => {
    const detail = buildDay(MONDAY, data, [], at(2026, 8, 23, 12));
    const block = detail.blocks[0];
    expect(block.topPct).toBeCloseTo((600 / 1440) * 100);
    expect(block.heightPct).toBeCloseTo((60 / 1440) * 100);
  });

  it("keeps the small-hours block the compact week clips away", () => {
    const night = {
      ...data,
      busy: [{ start: at(2026, 8, 24, 2), end: at(2026, 8, 24, 3) }],
    };
    const detail = buildDay(MONDAY, night, [], at(2026, 8, 23, 12));
    expect(detail.blocks).toHaveLength(1);
    expect(detail.blocks[0].topPct).toBeCloseTo((120 / 1440) * 100);
  });

  it("marks now only on the day itself", () => {
    const noon = at(2026, 8, 24, 12);
    expect(buildDay(MONDAY, data, [], noon).nowPct).toBeCloseTo(50);
    expect(buildDay(at(2026, 8, 25), data, [], noon).nowPct).toBeNull();
  });

  it("carries the day's gaps and its working flag", () => {
    const detail = buildDay(MONDAY, data, [], at(2026, 8, 23, 12));
    expect(detail.working).toBe(true);
    expect(detail.gaps).toHaveLength(2);
    const saturday = buildDay(at(2026, 8, 29), data, [], at(2026, 8, 23, 12));
    expect(saturday.working).toBe(false);
    expect(saturday.gaps).toEqual([]);
  });
});

describe("echoOn and repeat projection", () => {
  // Real occurrence: Tuesday Aug 25 at 10:00, repeating daily.
  const task = {
    id: 1,
    title: "Ritual",
    scheduledTs: at(2026, 8, 25, 10),
    repeatDays: "1,2,3,4,5,6,7",
  };

  it("projects onto later matching days at the same wall-clock time", () => {
    const echo = echoOn(task, at(2026, 8, 26));
    expect(echo).toEqual({
      start: at(2026, 8, 26, 10),
      end: at(2026, 8, 26, 10, 30),
    });
  });

  it("never lands on or before the real occurrence", () => {
    expect(echoOn(task, at(2026, 8, 25))).toBeNull(); // its own day
    expect(echoOn(task, at(2026, 8, 24))).toBeNull(); // the day before
  });

  it("respects the weekday mask", () => {
    const wedFri = { ...task, repeatDays: "3,5" };
    expect(echoOn(wedFri, at(2026, 8, 26))).not.toBeNull(); // Wednesday
    expect(echoOn(wedFri, at(2026, 8, 27))).toBeNull(); // Thursday
    expect(echoOn(wedFri, at(2026, 8, 28))).not.toBeNull(); // Friday
  });

  it("is silent without a repeat", () => {
    expect(echoOn({ ...task, repeatDays: null }, at(2026, 8, 26))).toBeNull();
    expect(echoOn({ ...task, repeatDays: undefined }, at(2026, 8, 26))).toBeNull();
  });

  it("fills the week's later columns with echo blocks", () => {
    const { days } = buildWeek(
      { startTs: MONDAY, busy: [], hours: HOURS },
      [task],
      at(2026, 8, 24, 9),
    );
    // Mon: nothing. Tue: the real task. Wed..Sun: one echo each.
    expect(days[0].blocks).toHaveLength(0);
    expect(days[1].blocks.map((b) => b.kind)).toEqual(["task"]);
    for (const day of days.slice(2)) {
      expect(day.blocks.map((b) => b.kind)).toEqual(["echo"]);
      expect(day.blocks[0].title).toBe("Ritual");
    }
  });

  it("gives the expanded day its echo too", () => {
    const detail = buildDay(
      at(2026, 8, 27),
      { busy: [], hours: HOURS },
      [task],
      at(2026, 8, 24, 9),
    );
    expect(detail.blocks.map((b) => b.kind)).toEqual(["echo"]);
    expect(detail.blocks[0].start).toBe(at(2026, 8, 27, 10));
  });
});

describe("buildWeek", () => {
  const data = {
    startTs: MONDAY,
    busy: [{ start: at(2026, 8, 24, 10), end: at(2026, 8, 24, 11) }],
    hours: HOURS,
  };

  it("marks working and non-working days from Monday-first hours", () => {
    const { days } = buildWeek(data, [], at(2026, 8, 24, 12));
    expect(days.map((d) => d.working)).toEqual([
      true, true, true, true, true, false, false,
    ]);
  });

  it("overlays a task on its own busy footprint with identical geometry, task last", () => {
    // The exact-start overlap: Threshold's event appears in Google's busy too.
    const taskStart = at(2026, 8, 24, 10);
    const withTaskBusy = {
      ...data,
      busy: [{ start: taskStart, end: taskStart + 1800 }],
    };
    const { days } = buildWeek(
      withTaskBusy,
      [{ id: 7, title: "Deep work", scheduledTs: taskStart }],
      at(2026, 8, 24, 9),
    );
    const blocks = days[0].blocks;
    expect(blocks).toHaveLength(2);
    expect(blocks[0].kind).toBe("busy");
    expect(blocks[1].kind).toBe("task");
    expect(blocks[1].topPct).toBeCloseTo(blocks[0].topPct);
    expect(blocks[1].heightPct).toBeCloseTo(blocks[0].heightPct);
    expect(blocks[1].taskId).toBe(7);
  });

  it("ignores a task scheduled outside the window", () => {
    const { days } = buildWeek(
      data,
      [{ id: 9, title: "Later", scheduledTs: at(2026, 9, 10, 10) }],
      at(2026, 8, 24, 12),
    );
    expect(days.every((d) => d.blocks.every((b) => b.kind !== "task"))).toBe(true);
  });

  it("draws the now line only on today, only on-axis", () => {
    const noonMonday = at(2026, 8, 24, 12);
    const { days } = buildWeek(data, [], noonMonday);
    expect(days[0].nowPct).toBeCloseTo(((720 - 420) / 780) * 100);
    expect(days.slice(1).every((d) => d.nowPct === null)).toBe(true);

    const smallHours = at(2026, 8, 24, 3);
    const early = buildWeek(data, [], smallHours);
    expect(early.days[0].nowPct).toBeNull();
  });

  it("splits a midnight-spanning busy block across two columns", () => {
    const spanning = {
      ...data,
      busy: [{ start: at(2026, 8, 24, 23), end: at(2026, 8, 25, 1) }],
    };
    // Full-day axis so the night hours are visible to the test.
    const { days } = buildWeek(
      { ...spanning, hours: { ...HOURS, startMin: 0, endMin: 0 } },
      [],
      at(2026, 8, 24, 12),
    );
    expect(days[0].blocks).toHaveLength(1);
    expect(days[1].blocks).toHaveLength(1);
    const monday = days[0].blocks[0];
    expect(monday.topPct + monday.heightPct).toBeCloseTo(100);
    expect(days[1].blocks[0].topPct).toBe(0);
  });
});
