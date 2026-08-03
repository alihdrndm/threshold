/**
 * Phase 3 (spec §5, §7): the intention popup rotates its look *and* its
 * interaction daily, because identical dialogs are neurologically habituated
 * within a handful of exposures. A theme is therefore not just colour — it
 * carries motion and copy variants too.
 *
 * Stubbed now so Phase 3 is additive.
 */

export type ThemeId = string;

export interface Theme {
  id: ThemeId;
  /** Value written to <html data-theme>; matched by a block in styles/index.css. */
  dataAttr: string;
  /** How the intention step asks for input on this theme's day. */
  inputMode: "type" | "click" | "slider";
  copy: {
    greeting: string;
    intentionPrompt: string;
    predictionPrompt: string;
  };
}

export const THEMES: readonly Theme[] = [];
