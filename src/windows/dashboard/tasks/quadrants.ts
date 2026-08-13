import { arrayMove } from "@dnd-kit/sortable";
import type { Task } from "@/lib/tauri";

/**
 * The Eisenhower matrix, expressed as the two flags actually stored.
 *
 * "Inbox" is not a fourth-and-a-half quadrant bolted on: it is the honest
 * representation of a task nobody has classified yet, which is both flags null.
 */
export type QuadrantId =
  | "inbox"
  | "do-first"
  | "schedule"
  | "delegate"
  | "eliminate";

export interface Quadrant {
  id: QuadrantId;
  label: string;
  /**
   * What the position means. The axis labels around the grid say this
   * visually now, so it renders for screen readers only — position on a 2x2
   * is exactly the information a linear reading loses.
   */
  hint: string;
  /**
   * What an empty zone invites. The one moment the quadrant's meaning is
   * worth a sentence is when you are deciding what belongs in it.
   */
  empty: string;
  urgent: boolean | null;
  important: boolean | null;
}

export const QUADRANTS: Quadrant[] = [
  {
    id: "do-first",
    label: "Do First",
    hint: "urgent and important",
    empty: "For what cannot wait",
    urgent: true,
    important: true,
  },
  {
    id: "schedule",
    label: "Schedule",
    hint: "important, not urgent",
    empty: "For what deserves a date",
    urgent: false,
    important: true,
  },
  {
    id: "delegate",
    label: "Delegate or shrink",
    hint: "urgent, not important",
    empty: "For what someone else can carry",
    urgent: true,
    important: false,
  },
  {
    id: "eliminate",
    label: "Eliminate",
    hint: "neither",
    empty: "For what you can let go",
    urgent: false,
    important: false,
  },
];

export const INBOX: Quadrant = {
  id: "inbox",
  label: "Inbox",
  hint: "unclassified",
  empty: "New tasks land here",
  urgent: null,
  important: null,
};

export function quadrantOf(task: Task): QuadrantId {
  if (task.urgent === null || task.important === null) return "inbox";
  if (task.urgent && task.important) return "do-first";
  if (!task.urgent && task.important) return "schedule";
  if (task.urgent && !task.important) return "delegate";
  return "eliminate";
}

export function quadrantById(id: QuadrantId): Quadrant {
  return QUADRANTS.find((q) => q.id === id) ?? INBOX;
}

/**
 * Where a task lives right now, the Done pile included.
 *
 * Search needs this where the board does not: on the matrix, position IS the
 * category, but a search result has left its position behind and must carry
 * the answer with it.
 */
export function placeOf(task: Task): { zone: QuadrantId | "done"; label: string } {
  if (task.status === "done") return { zone: "done", label: "Done" };
  const quadrant = quadrantById(quadrantOf(task));
  return { zone: quadrant.id, label: quadrant.label };
}

/** The board's own reading order - the tray, the four quadrants, Done last. */
export const PLACE_ORDER: readonly string[] = [
  "inbox",
  "do-first",
  "schedule",
  "delegate",
  "eliminate",
  "done",
];

/** Zone membership. The Inbox owns anything not yet fully classified. */
export function inQuadrant(task: Task, quadrant: Quadrant): boolean {
  return quadrant.urgent === null
    ? task.urgent === null || task.important === null
    : task.urgent === quadrant.urgent && task.important === quadrant.important;
}

/**
 * The target zone's order after a drop, or null when the drop changes nothing.
 *
 * `zone` is the target zone's tasks in their current order, including the moved
 * task when it already lives there. `overIndex` is the index of the card the
 * drop landed on, or -1 when it landed on the zone itself.
 *
 * Within a zone the moved card takes the slot of the card it landed on
 * (arrayMove semantics - the sortable preview has already shown exactly that).
 * Crossing zones there is no preview, so it lands before the card under the
 * cursor, or at the end when dropped on open space.
 */
export function orderAfterDrop(
  zone: Task[],
  moved: Task,
  overIndex: number,
): Task[] | null {
  const from = zone.findIndex((t) => t.id === moved.id);
  if (from !== -1) {
    if (overIndex === -1 || overIndex === from) return null;
    return arrayMove(zone, from, overIndex);
  }
  const next = [...zone];
  next.splice(overIndex === -1 ? next.length : overIndex, 0, moved);
  return next;
}

/**
 * Above this the UI shows a calm note. Deliberately a nudge and not a limit:
 * self-set rules get kept, imposed ones get worked around.
 */
export const DO_FIRST_SOFT_CAP = 3;
