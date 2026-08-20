/**
 * The repeat mask as the interface speaks it.
 *
 * Storage is the work_days vocabulary - "1,3,5", Mon=1..Sun=7 - shared with
 * the backend's parser, so one format crosses every boundary. Off is spelled
 * null everywhere: an empty set never round-trips as "".
 */

/** Monday-first, matching the mask's 1..7 and the Settings day pills. */
export const DAY_LABELS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/** "1,3,5" -> {1,3,5}. Ignores anything that is not a day 1-7. */
export function parseRepeat(days: string | null): Set<number> {
  const set = new Set<number>();
  if (!days) return set;
  for (const piece of days.split(",")) {
    const n = parseInt(piece.trim(), 10);
    if (n >= 1 && n <= 7) set.add(n);
  }
  return set;
}

/** Back to "1,3,5", sorted; null when empty. */
export function serializeRepeat(days: Set<number>): string | null {
  if (days.size === 0) return null;
  return [...days].sort((a, b) => a - b).join(",");
}

/** How the rule reads: "Every day", or the days by name. */
export function formatRepeat(days: string): string {
  const set = parseRepeat(days);
  if (set.size === 7) return "Every day";
  return [...set]
    .sort((a, b) => a - b)
    .map((n) => DAY_LABELS[n - 1])
    .join(", ");
}
