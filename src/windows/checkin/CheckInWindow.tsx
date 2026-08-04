import { useEffect, useState } from "react";
import {
  answerCheckin,
  dismissCheckin,
  pendingCheckin,
  type CheckinAnswer,
  type PendingCheckin,
} from "@/lib/tauri";
import { CHECKIN } from "./copy";

/**
 * What happened, asked once, quietly.
 *
 * The prediction this scores against is the only reason the ritual asks for one
 * at all — until now it was recorded and never compared to anything. Closing
 * without answering is a first-class outcome, recorded as its own value: a
 * question you cannot decline is a trap, and silence counted as failure would
 * corrupt the very measurement this exists to make.
 */
export function CheckInWindow() {
  const sessionId = Number(
    new URLSearchParams(window.location.search).get("session"),
  );
  const [subject, setSubject] = useState<PendingCheckin | null>(null);
  const [markDone, setMarkDone] = useState(false);
  const [busy, setBusy] = useState(false);

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

  async function answer(choice: CheckinAnswer) {
    if (busy || !subject) return;
    setBusy(true);
    await answerCheckin(subject.sessionId, choice, markDone).catch(() => {});
  }

  const blockMinutes = subject.blockHeldSecs
    ? Math.max(1, Math.ceil(subject.blockHeldSecs / 60))
    : null;

  return (
    <main className="checkin-card flex h-full flex-col justify-between gap-4 p-6 text-[var(--color-ink)]">
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
        {/* Only when the helper still owes time. Saying "you're free" while the
            sites are still blocked is the failure this app has already had. */}
        {blockMinutes !== null && (
          <p className="text-sm text-[var(--color-ink-muted)]">
            {CHECKIN.blockHeld(blockMinutes)}
          </p>
        )}
      </div>

      <div className="flex gap-2">
        {(["did_it", "partly", "no"] as const).map((choice) => (
          <button
            key={choice}
            type="button"
            onClick={() => void answer(choice)}
            className="ritual-pressable flex-1 rounded-full border border-[var(--color-border-subtle)] bg-[var(--color-fill-subtle)] px-3 py-2 text-sm focus-visible:outline-2 focus-visible:outline-offset-[3px] focus-visible:outline-[var(--color-accent)]"
          >
            {CHECKIN.answers[choice]}
          </button>
        ))}
      </div>

      <div className="flex items-center justify-between gap-3">
        {/* Offered only when the task still exists and is still open. Ticking a
            deleted one would put it back in the matrix. */}
        {subject.canMarkDone ? (
          <label className="flex items-center gap-2 text-sm text-[var(--color-ink-muted)]">
            <input
              type="checkbox"
              checked={markDone}
              onChange={(event) => setMarkDone(event.target.checked)}
              className="size-4 accent-[var(--color-accent)]"
            />
            {CHECKIN.markDone}
          </label>
        ) : (
          <span />
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
