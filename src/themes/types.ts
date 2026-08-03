/**
 * Phase 3 (spec §5, §7): the ritual rotates its look *and* its interaction
 * daily, because identical dialogs are neurologically habituated within a
 * handful of exposures — precisely the autopilot this app exists to interrupt.
 *
 * A theme is therefore not just colour. It carries how the questions are
 * phrased and how they are answered: some days you type, some days you pick.
 */

export type ThemeId = "ash" | "ember" | "tide" | "dusk";

/** How the intention is captured on a given day. */
export type IntentionMode = "type" | "choose";

/** How the commitment length is captured on a given day. */
export type DurationMode = "chips" | "slider";

export interface Theme {
  id: ThemeId;
  /** Written to <html data-theme>; matched by a block in styles/index.css. */
  dataAttr: ThemeId;
  intentionMode: IntentionMode;
  durationMode: DurationMode;
  copy: {
    greeting: string;
    intentionPrompt: string;
    predictionPrompt: string;
    ifThenPrompt: string;
    durationPrompt: string;
  };
}
