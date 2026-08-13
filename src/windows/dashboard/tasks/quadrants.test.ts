import { describe, expect, it } from "vitest";
import type { Task } from "@/lib/tauri";
import {
  INBOX,
  PLACE_ORDER,
  inQuadrant,
  orderAfterDrop,
  placeOf,
  quadrantById,
} from "./quadrants";

function task(id: number, flags?: { urgent: boolean; important: boolean }): Task {
  return {
    id,
    title: `task ${id}`,
    note: null,
    contextId: null,
    urgent: flags?.urgent ?? null,
    important: flags?.important ?? null,
    sortOrder: 0,
    status: "open",
    createdTs: "",
    completedTs: null,
  };
}

const ids = (list: Task[] | null) => list?.map((t) => t.id);

describe("inQuadrant", () => {
  it("gives the Inbox anything not fully classified", () => {
    expect(inQuadrant(task(1), INBOX)).toBe(true);
    expect(inQuadrant(task(2, { urgent: true, important: true }), INBOX)).toBe(
      false,
    );
  });

  it("matches a quadrant on both flags", () => {
    const doFirst = quadrantById("do-first");
    expect(inQuadrant(task(1, { urgent: true, important: true }), doFirst)).toBe(
      true,
    );
    expect(inQuadrant(task(2, { urgent: true, important: false }), doFirst)).toBe(
      false,
    );
  });
});

describe("placeOf", () => {
  it("names the quadrant a task sits in", () => {
    expect(placeOf(task(1, { urgent: true, important: true })).label).toBe(
      "Do First",
    );
    expect(placeOf(task(2)).label).toBe("Inbox");
  });

  it("puts a finished task in Done regardless of its flags", () => {
    const finished = { ...task(1, { urgent: true, important: true }), status: "done" as const };
    expect(placeOf(finished)).toEqual({ zone: "done", label: "Done" });
  });

  it("orders every place it can name", () => {
    // A zone missing from PLACE_ORDER would sort to the front via indexOf -1.
    expect(PLACE_ORDER).toContain(placeOf(task(1)).zone);
    expect(PLACE_ORDER).toContain(placeOf({ ...task(2), status: "done" as const }).zone);
  });
});

describe("orderAfterDrop", () => {
  const zone = [task(1), task(2), task(3)];

  it("moves a card onto the slot of the card it lands on", () => {
    // Dragging 1 down onto 3: the preview already showed 2 and 3 sliding up.
    expect(ids(orderAfterDrop(zone, zone[0], 2))).toEqual([2, 3, 1]);
    // And back up again.
    expect(ids(orderAfterDrop([task(2), task(3), task(1)], task(1), 0))).toEqual([
      1, 2, 3,
    ]);
  });

  it("says nothing when the drop changes nothing", () => {
    // Dropped on itself, or on the zone's own open space.
    expect(orderAfterDrop(zone, zone[1], 1)).toBeNull();
    expect(orderAfterDrop(zone, zone[1], -1)).toBeNull();
  });

  it("inserts an arriving card before the card under the cursor", () => {
    const arriving = task(9, { urgent: true, important: true });
    expect(ids(orderAfterDrop(zone, arriving, 1))).toEqual([1, 9, 2, 3]);
  });

  it("appends an arriving card dropped on open space", () => {
    const arriving = task(9);
    expect(ids(orderAfterDrop(zone, arriving, -1))).toEqual([1, 2, 3, 9]);
    expect(ids(orderAfterDrop([], arriving, -1))).toEqual([9]);
  });
});
