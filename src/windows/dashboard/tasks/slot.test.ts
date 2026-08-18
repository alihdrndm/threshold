import { describe, expect, it } from "vitest";
import { formatSlot, fromLocalInput, toLocalInput } from "./slot";

// A fixed "now": Monday 2024-06-03, 10:00 local.
const NOW = new Date(2024, 5, 3, 10, 0);
const at = (y: number, mo: number, d: number, h: number, mi: number) =>
  Math.round(new Date(y, mo, d, h, mi).getTime() / 1000);

describe("formatSlot", () => {
  it("names today and tomorrow", () => {
    expect(formatSlot(at(2024, 5, 3, 14, 0), NOW)).toMatch(/^Today /);
    expect(formatSlot(at(2024, 5, 4, 9, 15), NOW)).toMatch(/^Tomorrow /);
  });

  it("uses a weekday within the coming week", () => {
    // 2024-06-06 is a Thursday, three days out.
    expect(formatSlot(at(2024, 5, 6, 9, 0), NOW)).toMatch(/^Thu /);
  });

  it("uses a date further out", () => {
    // Ten days out — a month/day label, not a weekday.
    const label = formatSlot(at(2024, 5, 13, 9, 0), NOW);
    expect(label).not.toMatch(/^(Today|Tomorrow|Mon|Tue|Wed|Thu|Fri|Sat|Sun) /);
    expect(label).toMatch(/Jun/);
  });

  it("says so when there is no date yet", () => {
    expect(formatSlot(null, NOW)).toBe("no date yet");
  });

  it("does not count a late-evening slot as tomorrow", () => {
    // 23:30 today is still today, not tomorrow.
    expect(formatSlot(at(2024, 5, 3, 23, 30), NOW)).toMatch(/^Today /);
  });
});

describe("datetime-local round trip", () => {
  it("formats and parses back to the same second", () => {
    const ts = at(2024, 5, 3, 14, 30);
    const str = toLocalInput(ts);
    expect(str).toBe("2024-06-03T14:30");
    expect(fromLocalInput(str)).toBe(ts);
  });

  it("handles empty and null", () => {
    expect(toLocalInput(null)).toBe("");
    expect(fromLocalInput("")).toBeNull();
  });
});
