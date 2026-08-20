/**
 * How a scheduled task's time reads on its card.
 *
 * Short and human: today and tomorrow are named, this week is a weekday, and
 * anything further carries its date. Twelve-hour or twenty-four follows the
 * user's locale, which is what the calendar they came from does too. Pure, so
 * the awkward cases (midnight, month boundaries, "no date yet") are tested
 * rather than discovered.
 */

/** Local midnight for a given instant, as ms. */
function dayStart(ms: number): number {
  const d = new Date(ms);
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

const DAY = 86_400_000;

/**
 * The card label for a slot. `scheduledTs` is unix seconds, or null when the
 * task is in Schedule but has no event yet. `repeating` appends the ↻ that
 * says "this one comes back" - even to "no date yet", where it is the only
 * sign the rule exists.
 */
export function formatSlot(
  scheduledTs: number | null,
  now: Date,
  repeating = false,
): string {
  const mark = repeating ? " ↻" : "";
  if (scheduledTs === null) return `no date yet${mark}`;
  const when = new Date(scheduledTs * 1000);
  const time = when.toLocaleTimeString(undefined, {
    hour: "numeric",
    minute: "2-digit",
  });
  const days = Math.round((dayStart(when.getTime()) - dayStart(now.getTime())) / DAY);

  if (days === 0) return `Today ${time}${mark}`;
  if (days === 1) return `Tomorrow ${time}${mark}`;
  // Within the coming week, a weekday is clearer than a date.
  if (days > 1 && days < 7) {
    return `${when.toLocaleDateString(undefined, { weekday: "short" })} ${time}${mark}`;
  }
  return `${when.toLocaleDateString(undefined, { month: "short", day: "numeric" })} ${time}${mark}`;
}

/**
 * A slot as the value a `<input type="datetime-local">` wants: local
 * `YYYY-MM-DDTHH:mm`, no timezone. `null`/absent → empty.
 */
export function toLocalInput(scheduledTs: number | null): string {
  if (scheduledTs === null) return "";
  const d = new Date(scheduledTs * 1000);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** A datetime-local string back to unix seconds, or null if unparseable. */
export function fromLocalInput(value: string): number | null {
  if (!value) return null;
  const ms = new Date(value).getTime();
  return Number.isFinite(ms) ? Math.round(ms / 1000) : null;
}
