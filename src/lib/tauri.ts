import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";

/**
 * The only module that talks to the Rust core. Keeping `invoke` calls behind
 * typed functions here means command-name typos surface at one call site
 * instead of being scattered string literals across the UI.
 */

export type Outcome = "completed" | "skipped" | "drifted" | "browsing";

export interface NewIntention {
  text: string | null;
  ifThen: string | null;
  predictedYes: boolean | null;
  durationMin: number | null;
  categories: string | null;
  trigger: string | null;
  outcome: Outcome;
}

export interface IntentionRow {
  id: number;
  ts: string;
  text: string | null;
  ifThen: string | null;
  predictedYes: boolean | null;
  durationMin: number | null;
  categories: string | null;
  trigger: string | null;
  outcome: string;
}

/** Rust uses snake_case field names; convert at the boundary, not everywhere. */
function toRust(intention: NewIntention) {
  return {
    text: intention.text,
    if_then: intention.ifThen,
    predicted_yes: intention.predictedYes,
    duration_min: intention.durationMin,
    categories: intention.categories,
    trigger: intention.trigger,
    outcome: intention.outcome,
  };
}

export function ping(): Promise<string> {
  return invoke<string>("ping");
}

export function appVersion(): Promise<string> {
  return getVersion();
}

/** Record the outcome and close the ritual. Every path through the popup ends here. */
export function finishRitual(intention: NewIntention): Promise<number> {
  return invoke<number>("finish_ritual", { intention: toRust(intention) });
}

/** Close the intention window. The honourable exit must never be more than this. */
export function dismissPopup(): Promise<void> {
  return invoke<void>("dismiss_popup");
}

export function intentionSuggestions(): Promise<string[]> {
  return invoke<string[]>("intention_suggestions");
}

export function recentIntentions(limit?: number): Promise<IntentionRow[]> {
  return invoke<IntentionRow[]>("recent_intentions", { limit: limit ?? null });
}

export function rememberedCategories(): Promise<string> {
  return invoke<string>("remembered_categories");
}

export function rememberCategories(categories: string): Promise<void> {
  return invoke<void>("remember_categories", { categories });
}

export function dataLocation(): Promise<string> {
  return invoke<string>("data_location");
}

export interface TaskContext {
  id: number;
  name: string;
  sortOrder: number;
}

export interface Task {
  id: number;
  title: string;
  note: string | null;
  contextId: number | null;
  /** Both null means the Inbox: not yet classified. */
  urgent: boolean | null;
  important: boolean | null;
  sortOrder: number;
  status: "open" | "done" | "archived" | "deleted";
  createdTs: string;
  completedTs: string | null;
}

export function listContexts(): Promise<TaskContext[]> {
  return invoke<TaskContext[]>("list_contexts");
}

export function listTasks(): Promise<Task[]> {
  return invoke<Task[]>("list_tasks");
}

export function addTask(
  title: string,
  contextId: number | null,
): Promise<number> {
  return invoke<number>("add_task", { task: { title, note: null, contextId } });
}

export function moveTask(
  id: number,
  urgent: boolean | null,
  important: boolean | null,
  sortOrder: number,
): Promise<void> {
  return invoke<void>("move_task", { id, urgent, important, sortOrder });
}

export function setTaskStatus(id: number, status: string): Promise<void> {
  return invoke<void>("set_task_status", { id, status });
}

export function focusOnTask(): Promise<void> {
  return invoke<void>("focus_on_task");
}

/** True when running inside the Tauri webview rather than a plain browser. */
export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}
