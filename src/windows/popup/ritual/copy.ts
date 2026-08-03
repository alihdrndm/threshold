/**
 * Copy that does not rotate. Per-day phrasing lives with the themes; what is
 * here is fixed because changing it would change what is being measured.
 *
 * Tone rules from the spec, which are load-bearing rather than stylistic: calm,
 * adult, slightly warm, zero exclamation marks. Nothing congratulates the user
 * for *stating* an intention — praise at that moment licenses the very
 * behaviour being avoided. Reward is reserved for finishing.
 */

export const IF_THEN_DEFAULTS = [
  "take one breath and return to my task",
  "write the urge on the scratchpad",
  "stand up for thirty seconds",
] as const;

export const DURATIONS = [25, 50, 90] as const;

export const MIN_DURATION = 10;
export const MAX_DURATION = 120;

export interface BlockCategory {
  id: string;
  label: string;
  detail: string;
}

/**
 * Categories, not a verdict. A hardcoded list of "bad sites" moralises and gets
 * ignored; what drives real reduction is the user classifying their own.
 */
export const CATEGORIES: BlockCategory[] = [
  { id: "social", label: "Social", detail: "facebook, instagram, x" },
  { id: "video", label: "Video", detail: "youtube, tiktok" },
  { id: "forums", label: "Forums", detail: "reddit" },
];

export function timeLabel(date: Date): string {
  return date.toLocaleString(undefined, {
    weekday: "long",
    hour: "numeric",
    minute: "2-digit",
  });
}
