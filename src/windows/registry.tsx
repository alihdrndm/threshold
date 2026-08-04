import type { JSX } from "react";
import { DashboardWindow } from "./dashboard/DashboardWindow";
import { PopupWindow } from "./popup/PopupWindow";

/**
 * One bundle serves every window. Rust creates windows with a label
 * ("main", "popup"); the entry point resolves that label to a root component
 * here. Hash routing stays free for navigation *within* a window (e.g. the
 * dashboard/tasks segmented control in Phase 5).
 */
const WINDOWS: Record<string, () => JSX.Element> = {
  main: DashboardWindow,
};

export function resolveWindow(label: string): () => JSX.Element {
  // Ritual windows carry a counter ("popup-3"): a window destroyed on the event
  // loop keeps its label briefly, so each replacement takes a fresh one rather
  // than waiting for its predecessor to let go.
  if (label.startsWith("popup")) return PopupWindow;
  return WINDOWS[label] ?? DashboardWindow;
}
