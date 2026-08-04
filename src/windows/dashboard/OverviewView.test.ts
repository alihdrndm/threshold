import { describe, expect, it } from "vitest";
import type { SessionRecord } from "@/lib/tauri";
import { calibrate, reclaimedMinutes, summarise } from "./OverviewView";
import type { IntentionRow } from "@/lib/tauri";

function session(over: Partial<SessionRecord> = {}): SessionRecord {
  return {
    startedTs: 1_000,
    endedTs: 1_000 + 25 * 60,
    durationMin: 25,
    predictedYes: true,
    state: "completed",
    ...over,
  };
}

function intention(over: Partial<IntentionRow> = {}): IntentionRow {
  return {
    id: 1,
    ts: new Date().toISOString(),
    text: "write the report",
    ifThen: null,
    predictedYes: true,
    durationMin: 25,
    categories: null,
    trigger: "manual",
    outcome: "completed",
    taskId: null,
    ...over,
  };
}

describe("reclaimedMinutes", () => {
  it("counts what was spent, not what was committed", () => {
    // The bug this replaced: a ninety-minute session abandoned after five
    // added ninety minutes, so the number went up whenever you *started*
    // something — the opposite of what it claims to measure.
    const abandoned = session({
      durationMin: 90,
      endedTs: 1_000 + 5 * 60,
      state: "partly",
    });
    expect(reclaimedMinutes([abandoned])).toBe(5);
  });

  it("never counts more than was committed", () => {
    // A session answered an hour late did not run for an extra hour: the clock
    // stopped when its time was up.
    const answeredLate = session({ durationMin: 25, endedTs: 1_000 + 90 * 60 });
    expect(reclaimedMinutes([answeredLate])).toBe(25);
  });

  it("ignores sessions nobody answered", () => {
    // Not reclaimed time — time we know nothing about. Guessing in the
    // flattering direction is how a mirror turns into a trophy.
    const states = ["unanswered", "lapsed", "missed", "ended_early"] as const;
    const sessions = states.map((state) => session({ state }));
    expect(reclaimedMinutes(sessions)).toBe(0);
  });

  it("counts a partly-done session for the time it actually ran", () => {
    expect(reclaimedMinutes([session({ state: "partly" })])).toBe(25);
  });

  it("ignores a session that is still running", () => {
    expect(reclaimedMinutes([session({ endedTs: null, state: "completed" })])).toBe(0);
  });

  it("is zero rather than NaN with nothing recorded", () => {
    expect(reclaimedMinutes([])).toBe(0);
  });
});

describe("calibrate", () => {
  it("counts only the times you said you would", () => {
    const sessions = [
      session({ predictedYes: true, state: "completed" }),
      session({ predictedYes: true, state: "missed" }),
      // Predicted no: a different question, not part of this denominator.
      session({ predictedYes: false, state: "completed" }),
    ];
    expect(calibrate(sessions)).toEqual({ said: 2, kept: 1 });
  });

  it("excludes sessions nobody answered", () => {
    // The reason `unanswered` is a separate state from `missed`: silence must
    // not be scored as a no.
    const sessions = [
      session({ state: "completed" }),
      session({ state: "unanswered" }),
      session({ state: "lapsed" }),
      session({ state: "ended_early" }),
    ];
    expect(calibrate(sessions)).toEqual({ said: 1, kept: 1 });
  });

  it("does not count partly as kept", () => {
    // Promoting it would make the number flattering rather than useful.
    const result = calibrate([session({ state: "partly" })]);
    expect(result).toEqual({ said: 1, kept: 0 });
  });

  it("reports nothing rather than dividing by zero", () => {
    expect(calibrate([])).toEqual({ said: 0, kept: 0 });
  });

  it("ignores sessions with no prediction", () => {
    expect(calibrate([session({ predictedYes: null })])).toEqual({
      said: 0,
      kept: 0,
    });
  });
});

describe("summarise", () => {
  const day = 86_400_000;
  const daysAgo = (n: number) => new Date(Date.now() - n * day).toISOString();

  it("forgives a single missed day", () => {
    // Habit automaticity survives the odd gap; zeroing a streak for one miss is
    // what makes people abandon the whole thing.
    const rows = [
      intention({ id: 1, ts: daysAgo(0) }),
      // nothing yesterday
      intention({ id: 2, ts: daysAgo(2) }),
      intention({ id: 3, ts: daysAgo(3) }),
    ];
    const result = summarise(rows);
    expect(result.streak).toBe(3);
    expect(result.missedYesterday).toBe(true);
  });

  it("stops at two consecutive misses", () => {
    const rows = [
      intention({ id: 1, ts: daysAgo(0) }),
      // two days missing
      intention({ id: 2, ts: daysAgo(3) }),
    ];
    expect(summarise(rows).streak).toBe(1);
  });

  it("does not treat today as a miss while it is still running", () => {
    const rows = [intention({ id: 1, ts: daysAgo(1) })];
    const result = summarise(rows);
    expect(result.streak).toBe(1);
    expect(result.missedYesterday).toBe(false);
  });

  it("separates completed from browsing and drifted", () => {
    const rows = [
      intention({ id: 1, outcome: "completed" }),
      intention({ id: 2, outcome: "browsing" }),
      intention({ id: 3, outcome: "browsing" }),
      intention({ id: 4, outcome: "drifted" }),
    ];
    const result = summarise(rows);
    expect(result.completed).toBe(1);
    expect(result.browsing).toBe(2);
    expect(result.drifted).toBe(1);
  });
});
