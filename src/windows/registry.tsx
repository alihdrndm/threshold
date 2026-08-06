import type { JSX } from "react";
import { BlockedWindow } from "./blocked/BlockedWindow";
import { CheckInWindow } from "./checkin/CheckInWindow";
import { DashboardWindow } from "./dashboard/DashboardWindow";
import { PopupWindow } from "./popup/PopupWindow";

/**
 * One bundle serves every window. Rust creates windows with a label
 * ("main", "popup"); the entry point resolves that label to a root component
 * here. Hash routing stays free for navigation *within* a window (e.g. the
 * dashboard/tasks segmented control in Phase 5).
 */
// `| null` because a window may render nothing on its first frame — the
// check-in has no content until it has read which session it is asking about.
const WINDOWS: Record<string, () => JSX.Element | null> = {
  main: DashboardWindow,
  checkin: CheckInWindow,
  // Missing entries fall through to the dashboard below, which would render the
  // whole thing inside a 420px corner window rather than failing visibly.
  blocked: BlockedWindow,
};

export function resolveWindow(label: string): () => JSX.Element | null {
  // Ritual windows carry a counter ("popup-3"): a window destroyed on the event
  // loop keeps its label briefly, so each replacement takes a fresh one rather
  // than waiting for its predecessor to let go.
  if (label.startsWith("popup")) return PopupWindow;
  return WINDOWS[label] ?? DashboardWindow;
}
