import { describe, expect, it } from "vitest";
import { minutesLeft } from "./useMinutesLeft";

describe("minutesLeft", () => {
  const now = 1_700_000_000_000;
  const at = (seconds: number) => now / 1000 + seconds;

  it("rounds up, so it never reads zero while time remains", () => {
    // One second left is still "1 minute". Reading 0 with the session running
    // makes the banner look broken.
    expect(minutesLeft(at(1), now)).toBe(1);
    expect(minutesLeft(at(59), now)).toBe(1);
    expect(minutesLeft(at(61), now)).toBe(2);
  });

  it("is zero once the time is up", () => {
    expect(minutesLeft(at(0), now)).toBe(0);
  });

  it("never goes negative, however late the clock is read", () => {
    // The window can be hidden for hours; a stale timer must not produce
    // "-97 minutes left".
    expect(minutesLeft(at(-6_000), now)).toBe(0);
  });

  it("is derived from the deadline rather than counted down", () => {
    // The property that makes it survive sleep: the same deadline read at two
    // different moments gives two different answers, with no state in between.
    const endsTs = at(25 * 60);
    expect(minutesLeft(endsTs, now)).toBe(25);
    expect(minutesLeft(endsTs, now + 10 * 60_000)).toBe(15);
    expect(minutesLeft(endsTs, now + 3 * 3_600_000)).toBe(0);
  });
});
