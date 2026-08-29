import { describe, expect, it } from "vitest";
import { dueLabel } from "./due";

const AT = 1_700_000_000; // an arbitrary slot, unix seconds
const ms = (offsetSecs: number) => (AT + offsetSecs) * 1000;

describe("dueLabel", () => {
  it("counts down in minutes", () => {
    expect(dueLabel(AT, ms(-10 * 60))).toBe("in 10 min");
    expect(dueLabel(AT, ms(-90))).toBe("in 2 min");
  });

  it("says now for the minute either side of the slot", () => {
    expect(dueLabel(AT, ms(-60))).toBe("now");
    expect(dueLabel(AT, ms(0))).toBe("now");
    expect(dueLabel(AT, ms(60))).toBe("now");
  });

  it("owns up to being late", () => {
    expect(dueLabel(AT, ms(5 * 60))).toBe("5 min ago");
  });

  it("switches to hours past sixty minutes, dropping a zero remainder", () => {
    expect(dueLabel(AT, ms(-65 * 60))).toBe("in 1 h 5 min");
    expect(dueLabel(AT, ms(-120 * 60))).toBe("in 2 h");
  });
});
