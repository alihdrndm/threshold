import { useEffect, useState } from "react";
import {
  answerCheckin,
  continueSession,
  dismissCheckin,
  pendingCheckin,
  scheduleTask,
  startAgain,
  type CheckinAnswer,
  type PendingCheckin,
} from "@/lib/tauri";
import { DURATIONS } from "@/windows/popup/ritual/copy";
import { CHECKIN } from "./copy";

/**
 * What happened, asked once, quietly - and then, when something is left
 * undone, what next.
 *
 * "Did it" closes on the spot, as before. "Partly" and "Not this time" each
 * ask one more question, because the moment of admitting there is something
 * left is exactly when a next step costs least: keep going now (another slice,
 * same selections, only the length asked), start it over (the full ritual,
 * every option afresh), or give the task a date in Schedule. Every road keeps
 * a plain "just note it" - the check-in is a question, never a commitment
 * machine, and the answer must never be held hostage by the follow-up.
 *
 * The answer is recorded when a road is chosen, not when the first button is
 * pressed: Escape mid-question stays a dismissal, recorded as unanswered,
 * exactly as if the follow-up did not exist.
 */
type Step = "ask" | "partly" | "again" | "time";

export function CheckInWindow() {
  const sessionId = Number(
    new URLSearchParams(window.location.search).get("session"),
  );
  const [subject, setSubject] = useState<PendingCheckin | null>(null);
  const [markDone, setMarkDone] = useState(false);
  const [step, setStep] = useState<Step>("ask");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    pendingCheckin()
      .then(setSubject)
      .catch(() => void dismissCheckin(sessionId));
  }, [sessionId]);

  // Escape closes. The window deliberately does not take focus — tao's focus
  // fallback injects a synthetic keypress that opens the Start menu — so this
  // only binds after a click, which is why the visible "Not now" is mandatory
  // rather than a nicety.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") void dismissCheckin(sessionId);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [sessionId]);

  // No skeleton: this is on screen for a frame.
  if (!subject) return null;

  /** One action at a time; failures land in the error line, not in silence. */
  function act(action: () => Promise<void>) {
    if (busy) return;
    setBusy(true);
    setError(null);
    action()
      .then(() => setBusy(false))
      .catch((err: unknown) => {
        setBusy(false);
        setError(err instanceof Error ? err.message : String(err));
      });
  }

  const recordAndClose = (answer: CheckinAnswer, done = false) =>
    act(async () => {
      if (!subject) return;
      await answerCheckin(subject.sessionId, answer, done);
    });

  /** Schedule first, answer second: the answering closes the window. */
  const schedule = (answer: CheckinAnswer) =>
    act(async () => {
      if (!subject?.taskId) return;
      await scheduleTask(subject.taskId);
      await answerCheckin(subject.sessionId, answer, false);
    });

  /** Ritual first, answer second: a refusal keeps the question and its reason. */
  const again = () =>
    act(async () => {
      if (!subject) return;
      const result = await startAgain(subject.sessionId);
      if (!result.opened) {
        setError(result.reason ?? "The ritual could not be opened.");
        return;
      }
      await answerCheckin(subject.sessionId, "no", false);
    });

  const keepGoing = (minutes: number) =>
    act(async () => {
      if (!subject) return;
      await continueSession(subject.sessionId, "partly", minutes);
    });

  const blockMinutes = subject.blockHeldSecs
    ? Math.max(1, Math.ceil(subject.blockHeldSecs / 60))
    : null;

  // Scheduling needs a task that still exists and is still open; canMarkDone
  // is already exactly that fact.
  const canSchedule = subject.taskId !== null && subject.canMarkDone;

  const followUp = (
    headline: string,
    lead: { label: string; onPress: () => void },
  ) => (
    <>
      <div className="flex flex-col gap-1.5">
        <p className="text-base">{headline}</p>
        {error && (
          <p className="text-sm text-[var(--color-ink-muted)]">{error}</p>
        )}
      </div>
      <div className="flex gap-2">
        <FollowButton onPress={lead.onPress}>{lead.label}</FollowButton>
        {canSchedule && (
          <FollowButton
            onPress={() => schedule(step === "again" ? "no" : "partly")}
          >
            {CHECKIN.schedule}
          </FollowButton>
        )}
      </div>
    </>
  );

  return (
    <main className="checkin-card flex h-full flex-col justify-between gap-4 p-6 text-[var(--color-ink)]">
      {step === "ask" && (
        <>
          <div className="flex flex-col gap-1.5">
            <p className="text-base">
              {CHECKIN.headline(subject.minutes, subject.subject)}
            </p>
            <p className="text-sm text-[var(--color-ink-muted)]">
              {subject.predictedYes === null
                ? CHECKIN.noPrediction
                : subject.predictedYes
                  ? CHECKIN.predictedYes
                  : CHECKIN.predictedNo}
            </p>
            {/* Only when the helper still owes time. Saying "you're free" while
                the sites are still blocked is the failure this app has already
                had. */}
            {blockMinutes !== null && (
              <p className="text-sm text-[var(--color-ink-muted)]">
                {CHECKIN.blockHeld(blockMinutes)}
              </p>
            )}
          </div>

          <div className="flex gap-2">
            {(["did_it", "partly", "no"] as const).map((choice) => (
              <FollowButton
                key={choice}
                onPress={() => {
                  if (choice === "did_it") recordAndClose("did_it", markDone);
                  else setStep(choice === "partly" ? "partly" : "again");
                }}
              >
                {CHECKIN.answers[choice]}
              </FollowButton>
            ))}
          </div>
        </>
      )}

      {step === "partly" &&
        followUp(CHECKIN.partly.headline, {
          label: CHECKIN.partly.keepGoing,
          onPress: () => setStep("time"),
        })}

      {step === "again" &&
        followUp(CHECKIN.again.headline, {
          label: CHECKIN.again.startAgain,
          onPress: again,
        })}

      {step === "time" && (
        <>
          <div className="flex flex-col gap-1.5">
            <p className="text-base">{CHECKIN.timeHeadline}</p>
            {error && (
              <p className="text-sm text-[var(--color-ink-muted)]">{error}</p>
            )}
          </div>
          {/* The ritual's own lengths - the one question asked again, answered
              in the same vocabulary. Everything else rides along unchanged. */}
          <div className="flex gap-2">
            {DURATIONS.map((minutes) => (
              <FollowButton key={minutes} onPress={() => keepGoing(minutes)}>
                {CHECKIN.minutesLabel(minutes)}
              </FollowButton>
            ))}
          </div>
        </>
      )}

      <div className="flex items-center justify-between gap-3">
        {/* Offered only while the first question is up: continuing, retrying
            and scheduling all say the task is not done. Ticking a deleted one
            would put it back in the matrix, hence canMarkDone. */}
        {step === "ask" && subject.canMarkDone ? (
          <label className="flex items-center gap-2 text-sm text-[var(--color-ink-muted)]">
            <input
              type="checkbox"
              checked={markDone}
              onChange={(event) => setMarkDone(event.target.checked)}
              className="size-4 accent-[var(--color-accent)]"
            />
            {CHECKIN.markDone}
          </label>
        ) : step === "ask" ? (
          <span />
        ) : (
          // The plain road: record the answer already given and close. Every
          // follow-up keeps it, so the question is never held hostage.
          <button
            type="button"
            onClick={() => recordAndClose(step === "again" ? "no" : "partly")}
            className="ritual-exit rounded-full px-3 py-1.5 text-sm text-[var(--color-ink-muted)]"
          >
            {CHECKIN.justNote}
          </button>
        )}

        <button
          type="button"
          onClick={() => void dismissCheckin(subject.sessionId)}
          className="ritual-exit rounded-full px-3 py-1.5 text-sm text-[var(--color-ink-muted)]"
        >
          {CHECKIN.dismiss}
        </button>
      </div>
    </main>
  );
}

/** The check-in's one button shape, shared by every step. */
function FollowButton({
  children,
  onPress,
}: {
  children: React.ReactNode;
  onPress: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onPress}
      className="ritual-pressable flex-1 rounded-full border border-[var(--color-border-subtle)] bg-[var(--color-fill-subtle)] px-3 py-2 text-sm focus-visible:outline-2 focus-visible:outline-offset-[3px] focus-visible:outline-[var(--color-accent)]"
    >
      {children}
    </button>
  );
}
