/**
 * Light, dark, or whatever Windows is doing.
 *
 * A second axis, deliberately separate from the ritual's daily rotation in
 * `src/themes`. The two cannot fight: this one writes `data-appearance` and is
 * only ever mounted by the dashboard, while the rotation writes `data-theme`
 * and only ever runs in a ritual window. The stylesheet enforces the separation
 * regardless, by resetting to the dark palette for any document carrying a
 * rotating theme.
 */

export type Appearance = "dark" | "light" | "system";
export type Resolved = "dark" | "light";

/** The settings-table key, and the localStorage key that mirrors it. */
export const APPEARANCE_KEY = "appearance";

/** System, because the right default is the one the user already chose once. */
export const DEFAULT_APPEARANCE: Appearance = "system";

export function isAppearance(value: unknown): value is Appearance {
  return value === "dark" || value === "light" || value === "system";
}

/**
 * The same read the inline bootstrap in index.html performs, kept here so the
 * two cannot drift apart.
 */
export function readMirror(): Appearance {
  try {
    const stored = localStorage.getItem(APPEARANCE_KEY);
    return isAppearance(stored) ? stored : DEFAULT_APPEARANCE;
  } catch {
    return DEFAULT_APPEARANCE;
  }
}

export function writeMirror(preference: Appearance): void {
  try {
    localStorage.setItem(APPEARANCE_KEY, preference);
  } catch {
    // Private mode or a locked-down webview. The database still has it; the
    // only cost is one frame of the wrong scheme on the next cold start.
  }
}

export function systemPrefersDark(): boolean {
  return (
    typeof window !== "undefined" &&
    window.matchMedia("(prefers-color-scheme: dark)").matches
  );
}

export function resolve(preference: Appearance, systemDark: boolean): Resolved {
  if (preference === "system") return systemDark ? "dark" : "light";
  return preference;
}

/**
 * Apply the scheme to the document.
 *
 * Transitions are suppressed for exactly one frame while switching. Roughly
 * half the surfaces in the app carry a colour transition and half do not, so
 * letting the swap animate produces a stagger that reads as a rendering fault
 * rather than as a deliberate crossfade.
 */
export function apply(next: Resolved): void {
  const root = document.documentElement;
  if (root.dataset.appearance === next) return;

  root.dataset.appearanceSwitching = "";
  root.dataset.appearance = next;

  // Two frames: one for the attribute to take effect, one for the styles it
  // changed to be committed.
  requestAnimationFrame(() => {
    requestAnimationFrame(() => {
      delete root.dataset.appearanceSwitching;
    });
  });
}
