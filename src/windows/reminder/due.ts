/**
 * The pure part of the reminder: how "when" reads on the card.
 *
 * Kept apart from the window so the awkward cases - the minute either side of
 * due, the hour boundary, a snooze that lands after the slot - are tested
 * rather than discovered.
 */

/**
 * The relative half of the context line. Within a minute either side of the
 * slot it says "now": a countdown that flips from "in 1 min" to "1 min ago"
 * with nothing in between reads as a stopwatch, and this is a knock.
 */
export function dueLabel(scheduledTs: number, nowMs: number): string {
  const diffMs = scheduledTs * 1000 - nowMs;
  if (Math.abs(diffMs) <= 60_000) return "now";
  const minutes = Math.round(Math.abs(diffMs) / 60_000);
  const span =
    minutes >= 60
      ? minutes % 60 === 0
        ? `${Math.floor(minutes / 60)} h`
        : `${Math.floor(minutes / 60)} h ${minutes % 60} min`
      : `${minutes} min`;
  return diffMs > 0 ? `in ${span}` : `${span} ago`;
}

/**
 * The slot's clock time, in the locale's own 12/24-hour habit - the same call
 * the card label uses, so both surfaces name the moment identically.
 */
export function slotClock(scheduledTs: number): string {
  return new Date(scheduledTs * 1000).toLocaleTimeString(undefined, {
    hour: "numeric",
    minute: "2-digit",
  });
}
