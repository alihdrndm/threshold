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

/**
 * The words the user chose to meet at the honourable exit.
 *
 * Not the app moralising — the same rule the quote reservoir runs on: a
 * line you picked yourself is not the app talking, and it is doing a
 * different job. Nothing here blocks or delays; the browsing is recorded
 * either way and the way on is one click.
 *
 * Set exactly as given. Only the line breaks are ours, because the five
 * are five and the eye should be able to stack them.
 */
export const BROWSING_WORDS = {
  leadIn:
    "Narrated Ibn Abbas, who said: The Messenger of Allah ﷺ said to a man while advising him:",
  saying: "Take advantage of five things before they’re gone:",
  five: [
    "your youth before your old age,",
    "your health before your illness,",
    "your wealth before your poverty,",
    "your free time before you become occupied,",
    "and your life before your death.",
  ],
  source: "Al-Mustadrak ‘ala al-Sahihayn (7846)",
} as const;

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

/**
 * The express screen: one screen, because you already decided.
 *
 * The prediction and if-then prompts are deliberately absent here — those come
 * from the theme, word for word, so the two paths keep measuring the same
 * thing. A softer express wording would make their data non-comparable.
 */
export const EXPRESS = {
  eyebrow: "Focus",
  predictYes: "Probably",
  predictNo: "Probably not",
  categories: "Quiet these while you work",
  // Says what pressing it does, and gives one last chance to notice the
  // duration is wrong.
  start: (minutes: number) => `Start ${minutes} minutes`,
  exit: "Not right now",
} as const;

export function timeLabel(date: Date): string {
  return date.toLocaleString(undefined, {
    weekday: "long",
    hour: "numeric",
    minute: "2-digit",
  });
}
