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
 * Above this the UI shows a calm note. Deliberately a nudge and not a limit:
 * self-set rules get kept, imposed ones get worked around.
 */
export const DO_FIRST_SOFT_CAP = 3;
