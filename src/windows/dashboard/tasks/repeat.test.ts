import { describe, expect, it } from "vitest";
import { formatRepeat, parseRepeat, serializeRepeat } from "./repeat";
import { formatSlot } from "./slot";

describe("parseRepeat / serializeRepeat", () => {
  it("round-trips a mask", () => {
    expect(serializeRepeat(parseRepeat("1,3,5"))).toBe("1,3,5");
  });

  it("sorts and dedupes on the way out", () => {
    expect(serializeRepeat(new Set([5, 3, 1, 3]))).toBe("1,3,5");
  });

  it("spells the empty set as null, both ways", () => {
    expect(parseRepeat(null).size).toBe(0);
    expect(serializeRepeat(new Set())).toBeNull();
  });

  it("ignores junk on the way in", () => {
    expect([...parseRepeat("0,2,8,mon")]).toEqual([2]);
  });
});

describe("formatRepeat", () => {
  it("names every day as one word", () => {
    expect(formatRepeat("1,2,3,4,5,6,7")).toBe("Every day");
  });

  it("names chosen days Monday-first", () => {
    expect(formatRepeat("5,1,3")).toBe("Mon, Wed, Fri");
    expect(formatRepeat("2")).toBe("Tue");
  });
});

describe("formatSlot's repeat marker", () => {
  const NOW = new Date(2024, 5, 3, 10, 0);
  const at = (d: number, h: number) =>
    Math.floor(new Date(2024, 5, d, h).getTime() / 1000);

  it("marks a repeating slot", () => {
    expect(formatSlot(at(3, 14), NOW, true)).toMatch(/↻$/);
    expect(formatSlot(at(3, 14), NOW, false)).not.toMatch(/↻/);
    expect(formatSlot(at(3, 14), NOW)).not.toMatch(/↻/);
  });

  it("marks even a dateless repeat", () => {
    expect(formatSlot(null, NOW, true)).toBe("no date yet ↻");
  });
});
