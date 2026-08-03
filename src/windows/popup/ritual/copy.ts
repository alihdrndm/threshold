/**
 * Every string in the ritual, in one place.
 *
 * Tone rules from the spec, which are not stylistic preferences but load-bearing:
 * calm, adult, slightly warm, zero exclamation marks. Nothing congratulates the
 * user for *stating* an intention — praise at that moment licenses the very
 * behaviour being avoided. Reward is reserved for finishing.
 */

export const GREETINGS = [
  "Fresh session.",
  "Back at it.",
  "Here again.",
  "New start.",
] as const;

export const IF_THEN_DEFAULTS = [
  "take one breath and return to my task",
  "write the urge on the scratchpad",
  "stand up for thirty seconds",
] as const;

export const DURATIONS = [25, 50, 90] as const;

export interface BlockCategory {
  id: string;
  label: string;
  detail: string;
}

/**
 * Categories, not a verdict. The spec is explicit that a hardcoded list of
 * "bad sites" moralises and gets ignored; what drives real reduction is the
 * user classifying their own.
 */
export const CATEGORIES: BlockCategory[] = [
  { id: "social", label: "Social", detail: "facebook, instagram, x" },
  { id: "video", label: "Video", detail: "youtube, tiktok" },
  { id: "forums", label: "Forums", detail: "reddit" },
];

export function greetingFor(date: Date): string {
  // Rotates by day so the opening line is not the same wallpaper every morning.
  const dayIndex = Math.floor(date.getTime() / 86_400_000);
  return GREETINGS[dayIndex % GREETINGS.length];
}

export function timeLabel(date: Date): string {
  return date.toLocaleString(undefined, {
    weekday: "long",
    hour: "numeric",
    minute: "2-digit",
  });
}
