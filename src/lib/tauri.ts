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
  /** The task this came from, when it came from one. */
  taskId: number | null;
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
  taskId: number | null;
}

/**
 * Rust uses snake_case field names; convert at the boundary, not everywhere.
 *
 * This list is written out by hand, which means a field added to `NewIntention`
 * and forgotten here is dropped in transit with no type error anywhere. If you
 * are adding one, you are in the right place.
 */
function toRust(intention: NewIntention) {
  return {
    text: intention.text,
    if_then: intention.ifThen,
    predicted_yes: intention.predictedYes,
    duration_min: intention.durationMin,
    categories: intention.categories,
    trigger: intention.trigger,
    outcome: intention.outcome,
    task_id: intention.taskId,
  };
}

export function ping(): Promise<string> {
  return invoke<string>("ping");
}

export function appVersion(): Promise<string> {
  return getVersion();
}

export interface BlockOutcome {
  blocked: boolean;
  reason: string | null;
}

export interface RitualResult {
  id: number;
  /** The session this ritual started, when it committed to a length. */
  sessionId: number | null;
  /** Present only when the session asked for sites to be blocked. */
  block: BlockOutcome | null;
}

/**
 * Record the outcome, arm any block, and close. Every path through the popup ends here.
 *
 * `customSites` rides alongside the intention rather than inside it: the
 * intention is an append-only record of what was said, and the sites are a
 * setting that outlives any one ritual.
 */
export function finishRitual(
  intention: NewIntention,
  customSites: string[] = [],
): Promise<RitualResult> {
  return invoke<RitualResult>("finish_ritual", {
    intention: toRust(intention),
    customSites: customSites.join(","),
  });
}

/** Close the intention window. The honourable exit must never be more than this. */
export function dismissPopup(): Promise<void> {
  return invoke<void>("dismiss_popup");
}

/** A chip on the intention step. `taskId` is null for chips drawn from history. */
export interface Suggestion {
  taskId: number | null;
  title: string;
}

export function intentionSuggestions(): Promise<Suggestion[]> {
  return invoke<Suggestion[]>("intention_suggestions");
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

/**
 * Sites the user added themselves, remembered like the category toggles.
 *
 * The three built-in categories are somebody else's idea of distraction; these
 * are theirs. Asking for them again every session would put the friction on
 * exactly the part worth keeping.
 */
export function rememberedSites(): Promise<string[]> {
  return invoke<string[]>("remembered_sites");
}

/** Returns the list as stored — tidied and de-duplicated, so the UI can trust it. */
export function rememberSites(sites: string[]): Promise<string[]> {
  return invoke<string[]>("remember_sites", { sites });
}

/**
 * Tidy and check one typed site without saving it.
 *
 * Resolves to the name as it will actually be blocked (`https://www.X.com/a` →
 * `x.com`), or rejects with the reason. Showing what it becomes matters as much
 * as accepting it: the chip has to be the truth, not an echo of the typing.
 */
export function checkSite(site: string): Promise<string> {
  return invoke<string>("check_site", { site });
}

/**
 * The quote reservoir: lines you keep, shown where you decide.
 *
 * The app's own copy is deliberately flat — nothing here congratulates you for
 * stating an intention, because praise at that moment licenses the behaviour
 * you were avoiding. That rule is about the *app* talking. A line you chose
 * yourself is not the app talking, and at the moment of an urge something you
 * already believe carries further than anything this program could think to say.
 */
export interface Quote {
  id: number;
  text: string;
  author: string | null;
}

/** Where a quote is shown. The two are chosen independently. */
export type QuoteSurface = "ritual" | "blocked";

export function listQuotes(): Promise<Quote[]> {
  return invoke<Quote[]>("list_quotes");
}

/** Returns the whole list as stored, so the UI never guesses at the result. */
export function addQuote(text: string, author: string | null): Promise<Quote[]> {
  return invoke<Quote[]>("add_quote", { text, author });
}

export function removeQuote(id: number): Promise<Quote[]> {
  return invoke<Quote[]>("remove_quote", { id });
}

/** Null when the reservoir is empty — the surface then shows nothing at all. */
export function quoteFor(surface: QuoteSurface): Promise<Quote | null> {
  return invoke<Quote | null>("quote_for", { surface });
}

/** Pin a quote to a surface, or pass null to shuffle. */
export function chooseQuote(
  surface: QuoteSurface,
  id: number | null,
): Promise<void> {
  return invoke<void>("choose_quote", { surface, id });
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

/**
 * Persist one zone's order: each id gets its index as its sort order. Scoped
 * to the ids given, so renumbering a quadrant leaves the other zones alone.
 */
export function reorderTasks(ids: number[]): Promise<void> {
  return invoke<void>("reorder_tasks", { ids });
}

export function setTaskStatus(id: number, status: string): Promise<void> {
  return invoke<void>("set_task_status", { id, status });
}

export interface FocusResult {
  opened: boolean;
  reason: string | null;
}

export function focusOnTask(taskId: number): Promise<FocusResult> {
  return invoke<FocusResult>("focus_on_task", { taskId });
}

export interface DiagnosticCheck {
  name: string;
  ok: boolean;
  detail: string;
}

export interface Diagnostics {
  checks: DiagnosticCheck[];
  healthy: boolean;
  needsRepair: boolean;
  paused: boolean;
}

export function getDiagnostics(): Promise<Diagnostics> {
  return invoke<Diagnostics>("diagnostics");
}

/** Re-register the elevated helper. Raises a UAC prompt. */
export function repairHelper(): Promise<void> {
  return invoke<void>("repair_helper");
}

/**
 * Lift a block that still has time owed on it.
 *
 * The commitment lock fails closed on purpose, so this is the only way out
 * before the time is up. It is recorded, as a count and nothing more.
 */
export function emergencyUnblock(): Promise<void> {
  return invoke<void>("emergency_unblock");
}

/**
 * Match the native titlebar to the chosen appearance.
 *
 * `null` means follow Windows, which also restores the webview's automatic
 * colour scheme so a "system" preference keeps tracking the OS live.
 */
export function setWindowTheme(theme: "light" | "dark" | null): Promise<void> {
  return invoke<void>("set_window_theme", { theme });
}

export interface ActiveSession {
  id: number;
  taskId: number | null;
  /** The task's title as it was when the session began. */
  subject: string | null;
  startedTs: number;
  /** Unix seconds. Authoritative — always derive the countdown from this. */
  endsTs: number;
  durationMin: number;
  /** Advisory only; goes stale the moment the machine sleeps. */
  secondsRemaining: number;
  blocking: boolean;
  categories: string[];
}

export interface SessionStatus {
  session: ActiveSession | null;
  /** Seconds the sites stay quiet, with or without a session behind it. */
  blockSeconds: number | null;
}

export function sessionStatus(): Promise<SessionStatus> {
  return invoke<SessionStatus>("session_status");
}

/**
 * A finished session, for the Overview.
 *
 * `state` is the raw string on purpose: "unanswered" and "missed" must stay
 * distinguishable, because walking away is not the same as saying no and
 * folding them together would corrupt the only self-report this app collects.
 */
export interface SessionRecord {
  startedTs: number;
  /** When it actually stopped; null while it is still running. */
  endedTs: number | null;
  /** What was committed to — not what was spent. */
  durationMin: number;
  predictedYes: boolean | null;
  state:
    | "running"
    | "awaiting_checkin"
    | "completed"
    | "partly"
    | "missed"
    | "ended_early"
    | "lapsed"
    | "unanswered";
}

export function recentSessions(limit?: number): Promise<SessionRecord[]> {
  return invoke<SessionRecord[]>("recent_sessions", { limit });
}

export interface EndSessionResult {
  /** Non-null means the sites stay blocked after the session ends. */
  blockHeldSecs: number | null;
}

export function endSessionEarly(sessionId: number): Promise<EndSessionResult> {
  return invoke<EndSessionResult>("end_session_early", { sessionId });
}

export interface PendingCheckin {
  sessionId: number;
  taskId: number | null;
  canMarkDone: boolean;
  subject: string | null;
  predictedYes: boolean | null;
  /** Minutes actually elapsed, not minutes committed. */
  minutes: number;
  lateBy: number;
  blockHeldSecs: number | null;
}

export type CheckinAnswer = "did_it" | "partly" | "no";

export function pendingCheckin(): Promise<PendingCheckin | null> {
  return invoke<PendingCheckin | null>("pending_checkin");
}

export function answerCheckin(
  sessionId: number,
  answer: CheckinAnswer,
  markTaskDone: boolean,
): Promise<void> {
  return invoke<void>("answer_checkin", { sessionId, answer, markTaskDone });
}

export function dismissCheckin(sessionId: number): Promise<void> {
  return invoke<void>("dismiss_checkin", { sessionId });
}

export interface ContinueResult {
  sessionId: number;
  /** Present when the last slice blocked or asked to; re-armed the same way. */
  block: BlockOutcome | null;
}

/**
 * "Keep going now": record the answer, then start another slice on the same
 * subject with the same selections - only the length is asked again.
 */
export function continueSession(
  sessionId: number,
  answer: CheckinAnswer,
  minutes: number,
): Promise<ContinueResult> {
  return invoke<ContinueResult>("continue_session", {
    sessionId,
    answer,
    minutes,
  });
}

/**
 * "Start it again": reopen the ritual for what this session was about, every
 * option asked afresh. Does not record the check-in answer - do that once
 * `opened` comes back true, so a refusal leaves the question and its reason.
 */
export function startAgain(sessionId: number): Promise<FocusResult> {
  return invoke<FocusResult>("start_again", { sessionId });
}

/** Give a task a date: move it to the Schedule quadrant, joining the end. */
export function scheduleTask(taskId: number): Promise<void> {
  return invoke<void>("schedule_task", { taskId });
}

/** Unix seconds a pause runs until, or null when nothing is paused. */
export function pauseStatus(): Promise<number | null> {
  return invoke<number | null>("pause_status");
}

export function pauseFor(days: number): Promise<number> {
  return invoke<number>("pause_for", { days });
}

export function resumeNow(): Promise<void> {
  return invoke<void>("resume_now");
}

export function getSettings(): Promise<[string, string][]> {
  return invoke<[string, string][]>("get_settings");
}

export function setSetting(key: string, value: string): Promise<void> {
  return invoke<void>("set_setting", { key, value });
}

/** True when running inside the Tauri webview rather than a plain browser. */
export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}
