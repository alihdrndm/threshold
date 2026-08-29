import { useEffect, useState } from "react";
import { motion, useReducedMotion } from "motion/react";
import {
  dismissReminder,
  getSettings,
  listContexts,
  listTasks,
  openDashboard,
  snoozeReminder,
  type Task,
} from "@/lib/tauri";
import { chime } from "@/windows/blocked/wall";
import { dueLabel, slotClock } from "./due";

/**
 * The slot's knock: the corner window Rust opens shortly before a scheduled
 * task is due.
 *
 * The check-in's shape, on purpose - same size, same corner, same rule that it
 * never takes focus (tao's focus fallback injects a keypress that opens the
 * Start menu), which is why every way out is a visible button and Escape only
 * works after a click. It stays up until acted on: the phone's version of this
 * notification persists, and a knock that gives up after ten seconds was never
 * a knock. Rust closes it unaided once the slot itself is over.
 *
 * Snoozing defers the knock, never the task - the slot and its calendar event
 * stay put, exactly like the snooze on the phone.
 */

/** The settings key for the chime. Absent means on: sound is the default. */
export const REMINDER_SOUND_KEY = "reminder_sound";

/** Google Calendar's own ladder, stopping where moving the slot is honester. */
const SNOOZE_MINUTES = [5, 10, 15, 30];

export function ReminderWindow() {
  const taskId = Number(
    new URLSearchParams(window.location.search).get("task"),
  );
  const [task, setTask] = useState<Task | null>(null);
  const [area, setArea] = useState<string | null>(null);
  const [nowMs, setNowMs] = useState(() => Date.now());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const reduce = useReducedMotion();

  useEffect(() => {
    Promise.all([listTasks(), listContexts()])
      .then(([tasks, contexts]) => {
        const found = tasks.find((t) => t.id === taskId);
        // The task moved on while the window was being built - completed,
        // rescheduled, dragged out of Schedule. Old news; let it go.
        const current =
          found &&
          found.status === "open" &&
          found.urgent === false &&
          found.important === true &&
          found.scheduledTs !== null;
        if (!current) {
          void dismissReminder(taskId);
          return;
        }
        setTask(found);
        setArea(
          contexts.find((c) => c.id === found.contextId)?.name ?? null,
        );
      })
      .catch(() => void dismissReminder(taskId));
  }, [taskId]);

  // The chime, once, as the card arrives - unless it has been turned off. Any
  // failure is silence: the sound is a courtesy on top of the reminder.
  useEffect(() => {
    if (!task) return;
    getSettings()
      .then((pairs) => {
        const off = pairs.some(
          ([k, v]) => k === REMINDER_SOUND_KEY && v === "off",
        );
        if (off || typeof AudioContext === "undefined") return;
        chime(new AudioContext());
      })
      .catch(() => {});
  }, [task]);

  // "in 10 min" ages while the window sits there; half a minute of drift is
  // fine for a line that speaks in minutes. No IPC in this timer.
  useEffect(() => {
    const tick = window.setInterval(() => setNowMs(Date.now()), 30_000);
    return () => window.clearInterval(tick);
  }, []);

  // Escape closes. The window deliberately does not take focus, so this only
  // binds after a click - the visible Dismiss is mandatory, not a nicety.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") void dismissReminder(taskId);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [taskId]);

  // No skeleton: this is on screen for a frame.
  if (!task) return null;

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

  const snooze = (minutes: number) =>
    act(() => snoozeReminder(taskId, minutes));

  /** The phone's "tap the notification": show me the task, then stop asking. */
  const show = () =>
    act(async () => {
      await openDashboard();
      await dismissReminder(taskId);
    });

  const context = [
    slotClock(task.scheduledTs as number),
    dueLabel(task.scheduledTs as number, nowMs),
    area,
  ]
    .filter(Boolean)
    .join(" · ");

  return (
    <motion.main
      className="checkin-card flex h-full flex-col justify-between gap-4 p-6 text-[var(--color-ink)]"
      initial={reduce ? false : { opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ type: "spring", duration: 0.45, bounce: 0 }}
    >
      <div className="flex flex-col gap-1.5">
        <button
          type="button"
          onClick={show}
          className="ritual-pressable line-clamp-2 text-left text-base"
          title="Open Threshold"
        >
          {task.title}
        </button>
        <p className="text-sm text-[var(--color-ink-muted)]">{context}</p>
        {error && (
          <p className="text-sm text-[var(--color-ink-muted)]">{error}</p>
        )}
      </div>

      <div className="flex flex-col gap-2">
        <p className="text-xs text-[var(--color-ink-muted)]">Snooze</p>
        <div className="flex gap-2">
          {SNOOZE_MINUTES.map((minutes) => (
            <SnoozeButton key={minutes} onPress={() => snooze(minutes)}>
              {minutes} min
            </SnoozeButton>
          ))}
        </div>
      </div>

      <div className="flex justify-end">
        <button
          type="button"
          onClick={() => void dismissReminder(taskId)}
          className="ritual-exit rounded-full px-3 py-1.5 text-sm text-[var(--color-ink-muted)]"
        >
          Dismiss
        </button>
      </div>
    </motion.main>
  );
}

/** The check-in's button shape, worn by the snooze ladder. */
function SnoozeButton({
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
