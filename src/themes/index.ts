import type { Theme } from "./types";

export * from "./types";

/**
 * Four days of variation. Phrasing shifts, the palette shifts, and the way you
 * answer shifts — but the questions themselves never do, because each one is
 * carrying a specific effect and rewording them into something softer would
 * quietly throw that away.
 */
export const THEMES: readonly Theme[] = [
  {
    id: "ash",
    dataAttr: "ash",
    intentionMode: "type",
    durationMode: "chips",
    copy: {
      greeting: "Fresh session.",
      intentionPrompt: "What are you here for?",
      predictionPrompt: "Will you start this before opening anything else?",
      ifThenPrompt: "If I feel the urge to open a feed, then I will…",
      durationPrompt: "How long?",
    },
  },
  {
    id: "ember",
    dataAttr: "ember",
    intentionMode: "choose",
    durationMode: "slider",
    copy: {
      greeting: "Back at it.",
      intentionPrompt: "What deserves this hour?",
      predictionPrompt: "Will you start this before opening anything else?",
      ifThenPrompt: "When the pull comes, then I will…",
      durationPrompt: "Commit how much?",
    },
  },
  {
    id: "tide",
    dataAttr: "tide",
    intentionMode: "type",
    durationMode: "slider",
    copy: {
      greeting: "Here again.",
      intentionPrompt: "What are you starting?",
      predictionPrompt: "Will you start this before opening anything else?",
      ifThenPrompt: "If a feed calls, then I will…",
      durationPrompt: "For how long?",
    },
  },
  {
    id: "dusk",
    dataAttr: "dusk",
    intentionMode: "choose",
    durationMode: "chips",
    copy: {
      greeting: "New start.",
      intentionPrompt: "Where is your attention going?",
      predictionPrompt: "Will you start this before opening anything else?",
      ifThenPrompt: "If I reach for a feed, then I will…",
      durationPrompt: "How long?",
    },
  },
];

/**
 * Rotate by local calendar day, so the theme is stable for a whole session and
 * differs tomorrow. Deriving it from the date rather than storing it means a
 * reinstall cannot accidentally serve the same day twice.
 */
export function themeFor(date: Date): Theme {
  const days = Math.floor(
    new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime() /
      86_400_000,
  );
  return THEMES[((days % THEMES.length) + THEMES.length) % THEMES.length];
}
