import { describe, expect, it } from "vitest";
import type { TaskContext } from "@/lib/tauri";
import { parseTitle, suggestAreas, tagAtCaret } from "./areas";

const AREAS: TaskContext[] = [
  { id: 1, name: "Job", sortOrder: 0 },
  { id: 2, name: "Personal", sortOrder: 1 },
  { id: 3, name: "Side", sortOrder: 2 },
];

describe("parseTitle", () => {
  it("files a task under the tagged area and drops the tag from the title", () => {
    const parsed = parseTitle("Call the bank #personal", AREAS);
    expect(parsed.title).toBe("Call the bank");
    expect(parsed.context?.name).toBe("Personal");
    expect(parsed.unknown).toBeNull();
  });

  it("matches regardless of case and position", () => {
    expect(parseTitle("#JOB write the report", AREAS).context?.id).toBe(1);
    expect(parseTitle("write #Side the report", AREAS).title).toBe("write the report");
  });

  it("reports a tag that names no area rather than swallowing it", () => {
    const parsed = parseTitle("Water the plants #home", AREAS);
    expect(parsed.title).toBe("Water the plants");
    expect(parsed.context).toBeNull();
    expect(parsed.unknown).toBe("home");
  });

  it("takes only the first tag - a task has one area", () => {
    const parsed = parseTitle("thing #job #personal", AREAS);
    expect(parsed.context?.name).toBe("Job");
    expect(parsed.title).toBe("thing #personal");
  });

  it("leaves a title without tags alone", () => {
    expect(parseTitle("Just a task", AREAS)).toEqual({
      title: "Just a task",
      context: null,
      unknown: null,
    });
    // A hash glued to a word is not a tag: "issue#12" stays as typed.
    expect(parseTitle("fix issue#12", AREAS).title).toBe("fix issue#12");
  });
});

describe("tagAtCaret", () => {
  it("finds the tag the caret is inside", () => {
    expect(tagAtCaret("call #pe", 8)).toEqual({ start: 5, query: "pe" });
    expect(tagAtCaret("#", 1)).toEqual({ start: 0, query: "" });
  });

  it("finds nothing when the caret is not in a tag", () => {
    expect(tagAtCaret("call #pe now", 12)).toBeNull();
    expect(tagAtCaret("plain", 5)).toBeNull();
  });
});

describe("suggestAreas", () => {
  it("offers areas by prefix, in toolbar order", () => {
    expect(suggestAreas("", AREAS).map((c) => c.name)).toEqual(["Job", "Personal", "Side"]);
    expect(suggestAreas("s", AREAS).map((c) => c.name)).toEqual(["Side"]);
    expect(suggestAreas("zz", AREAS)).toEqual([]);
  });
});
