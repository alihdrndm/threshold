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
  popup: PopupWindow,
};

export function resolveWindow(label: string): () => JSX.Element {
  return WINDOWS[label] ?? DashboardWindow;
}
