import { describe, expect, it, vi } from "vitest";
import { chime, displaySize, exitAt, lingerFrom, type AudioContextLike } from "./wall";

describe("displaySize", () => {
  it("sets a short line large and steps down as it lengthens", () => {
    expect(displaySize("The magic you're looking for is in the work you're avoiding.")).toBe("lg");
    expect(displaySize("a".repeat(90))).toBe("lg");
    expect(displaySize("a".repeat(91))).toBe("md");
    expect(displaySize("a".repeat(170))).toBe("md");
    expect(displaySize("a".repeat(171))).toBe("sm");
  });

  it("does not let padding whitespace inflate the count", () => {
    expect(displaySize("  " + "a".repeat(90) + "  ")).toBe("lg");
  });
});

describe("the wall's lifetime", () => {
  it("reads how long it has from the URL Rust opened it at", () => {
    expect(lingerFrom("?window=blocked&linger=12000")).toBe(12_000);
  });

  it("falls back rather than living forever or dying at once", () => {
    expect(lingerFrom("?window=blocked")).toBe(12_000);
    expect(lingerFrom("?linger=abc")).toBe(12_000);
    expect(lingerFrom("?linger=0")).toBe(12_000);
  });

  it("starts the exit fade so it ends as the window is destroyed", () => {
    expect(exitAt(12_000, 220)).toBe(11_780);
    // A lifetime shorter than the fade fades from the start, never negatively.
    expect(exitAt(100, 220)).toBe(0);
  });
});

describe("chime", () => {
  it("plays two notes a fifth apart, quietly, and stops them", () => {
    const started: number[] = [];
    const peaks: number[] = [];
    const stops: number[] = [];
    const context: AudioContextLike = {
      currentTime: 10,
      destination: {} as AudioNode,
      createOscillator: () =>
        ({
          type: "sine",
          frequency: { value: 0 },
          connect: vi.fn(),
          start: (at: number) => started.push(at),
          stop: (at: number) => stops.push(at),
        }) as unknown as OscillatorNode,
      createGain: () =>
        ({
          gain: {
            setValueAtTime: vi.fn(),
            exponentialRampToValueAtTime: (value: number) => peaks.push(value),
          },
          connect: vi.fn(),
        }) as unknown as GainNode,
    };

    chime(context);

    expect(started).toEqual([10, 10.14]);
    expect(stops.length).toBe(2);
    // The loudest any note gets. A tap on the shoulder, not an alarm.
    expect(Math.max(...peaks)).toBeLessThanOrEqual(0.1);
  });
});
