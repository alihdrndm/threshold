import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import clsx from "clsx";
import {
  buildDay,
  formatDuration,
  type Interval,
  type ScheduledTask,
} from "./week";

/**
 * One day, expanded: the whole 24 hours at readable scale, and the answer the
 * compact strip only implies - "Room", the day's free gaps inside working
 * hours, listed with their sizes. The compact week clips the night for
 * density; the point of expanding is honesty, so nothing here is clipped.
 *
 * The card does not appear - it grows out of the clicked column. The morph is
 * a WAAPI transform from the column's rectangle to the card's resting place
 * (transform-only, so it runs on the GPU), with the content fading in just
 * behind it. Closing reverses the journey, faster - the way everything in
 * this app leaves faster than it arrives. Reduced motion keeps the fades and
 * skips the travel.
 */

const OPEN_MS = 280;
const CLOSE_MS = 200;
const EASE = "cubic-bezier(0.23, 1, 0.32, 1)";

/** One hour of the day, in pixels. Tall enough that a half-hour block can
    carry its own time label; the whole day is 24 of these and scrolls. */
const HOUR_PX = 48;
const DAY_PX = 24 * HOUR_PX;

function reducedMotion(): boolean {
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

function clock(ts: number): string {
  return new Date(ts * 1000).toLocaleTimeString(undefined, {
    hour: "numeric",
    minute: "2-digit",
  });
}

function range(iv: { start: number; end: number }): string {
  return `${clock(iv.start)} – ${clock(iv.end)}`;
}

export function DayModal({
  dayStart,
  busy,
  tasks,
  hours,
  from,
  onClose,
}: {
  /** Local midnight of the day to expand, unix seconds. */
  dayStart: number;
  busy: Interval[];
  tasks: ScheduledTask[];
  hours: { startMin: number; endMin: number; days: boolean[] };
  /** The clicked column's rectangle - where the card grows from. */
  from: DOMRect | null;
  onClose: () => void;
}) {
  const card = useRef<HTMLDivElement>(null);
  const scrim = useRef<HTMLDivElement>(null);
  const closeButton = useRef<HTMLButtonElement>(null);
  const scroller = useRef<HTMLDivElement>(null);
  const [leaving, setLeaving] = useState(false);
  const now = Math.floor(Date.now() / 1000);

  const detail = useMemo(
    () => buildDay(dayStart, { busy, hours }, tasks, now),
    // `now` moves every render; the detail only needs the day's data.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [dayStart, busy, tasks, hours],
  );

  const date = new Date(dayStart * 1000);
  const isToday = new Date(now * 1000).toDateString() === date.toDateString();

  // The morph in: from the clicked column's rectangle to rest.
  useLayoutEffect(() => {
    const el = card.current;
    if (!el || !from || reducedMotion()) return;
    const to = el.getBoundingClientRect();
    if (to.width === 0 || to.height === 0) return;
    el.animate(
      [
        {
          transform: `translate(${from.left - to.left}px, ${from.top - to.top}px) scale(${from.width / to.width}, ${from.height / to.height})`,
        },
        { transform: "none" },
      ],
      { duration: OPEN_MS, easing: EASE },
    );
  }, [from]);

  const close = useCallback(() => {
    setLeaving((already) => {
      if (already) return already;
      const el = card.current;
      if (!el || !from || reducedMotion()) {
        // The fade alone: let the scrim/content keyframes play out.
        window.setTimeout(onClose, reducedMotion() ? 120 : 0);
        return true;
      }
      const to = el.getBoundingClientRect();
      const anim = el.animate(
        [
          { transform: "none", opacity: 1 },
          {
            transform: `translate(${from.left - to.left}px, ${from.top - to.top}px) scale(${from.width / to.width}, ${from.height / to.height})`,
            opacity: 0.2,
          },
        ],
        { duration: CLOSE_MS, easing: EASE, fill: "forwards" },
      );
      anim.finished.then(onClose).catch(() => onClose());
      // A suspended animation timeline (hidden window) must not strand the
      // modal: the deadline closes it whether or not the travel played.
      window.setTimeout(onClose, CLOSE_MS + 120);
      return true;
    });
  }, [from, onClose]);

  // Escape leaves; the close control takes focus on arrival so the keyboard
  // is already holding the way out.
  useEffect(() => {
    closeButton.current?.focus();
    const key = (event: KeyboardEvent) => {
      if (event.key === "Escape") close();
    };
    document.addEventListener("keydown", key);
    return () => document.removeEventListener("keydown", key);
  }, [close]);

  // Land the scroll where the day actually happens: on the now line for
  // today, at the working day's open otherwise - never at midnight.
  useLayoutEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const target =
      detail.nowPct !== null
        ? (detail.nowPct / 100) * DAY_PX - el.clientHeight / 3
        : (hours.startMin / 1440) * DAY_PX - HOUR_PX / 2;
    el.scrollTop = Math.max(0, target);
    // Once, on arrival.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // The list dedupes what the timeline overlays: a busy interval that is
  // exactly a task's slot is the task, not a second booking.
  const taskStarts = new Set(tasks.map((t) => t.scheduledTs));
  const booked = detail.blocks
    .filter(
      (b) =>
        b.kind === "task" ||
        !(taskStarts.has(b.start) && b.end - b.start === 1800),
    )
    .sort((a, b) => a.start - b.start);

  // Every hour named: the expansion's promise is explicitness.
  const ticks = Array.from({ length: 24 }, (_, i) => i * 60);

  return createPortal(
    <div
      ref={scrim}
      data-leaving={leaving || undefined}
      onPointerDown={(event) => {
        if (event.target === scrim.current) close();
      }}
      className="day-scrim fixed inset-0 z-[60] grid place-items-center p-6"
    >
      <div
        ref={card}
        role="dialog"
        aria-modal="true"
        aria-label={date.toLocaleDateString(undefined, {
          weekday: "long",
          month: "long",
          day: "numeric",
        })}
        className="day-modal flex h-[min(52rem,92vh)] w-[min(56rem,94vw)] flex-col overflow-hidden rounded-2xl border border-[var(--color-border-subtle)] bg-[var(--color-surface-raised)] shadow-[0_24px_64px_-16px_rgb(0_0_0/0.6)]"
      >
        <div className="day-modal-content flex min-h-0 flex-1 flex-col">
          <header className="flex items-baseline gap-3 px-5 pt-4 pb-3">
            <h2 className="text-sm tracking-[0.14em] uppercase text-[var(--color-ink)]">
              {isToday
                ? "Today"
                : date.toLocaleDateString(undefined, { weekday: "long" })}
            </h2>
            <span className="text-xs text-[var(--color-ink-muted)]">
              {date.toLocaleDateString(undefined, {
                month: "long",
                day: "numeric",
              })}
            </span>
            <button
              ref={closeButton}
              type="button"
              onClick={close}
              aria-label="Close"
              className="ml-auto grid h-7 w-7 place-items-center rounded-full text-[var(--color-ink-muted)] transition-colors duration-100 hover:bg-[var(--color-fill-selected)] hover:text-[var(--color-ink)] active:scale-95"
            >
              ×
            </button>
          </header>

          <div className="flex min-h-0 flex-1 gap-5 px-5 pb-5">
            {/* The whole day at a fixed hour scale; a tall day scrolls. The
                scroll arrives at now (today) or at the day's open, never at
                midnight. */}
            <div
              ref={scroller}
              className="min-w-0 flex-[2] overflow-y-auto overscroll-contain rounded-lg"
            >
              <div className="flex gap-1.5" style={{ height: DAY_PX }}>
              <div className="relative w-12 shrink-0" aria-hidden>
                {ticks.map((minute) => (
                  <span
                    key={minute}
                    className={clsx(
                      "absolute right-1 text-[10px] tabular-nums whitespace-nowrap text-[var(--color-ink-muted)]",
                      minute !== 0 && "-translate-y-1/2",
                    )}
                    style={{ top: `${(minute / 1440) * 100}%` }}
                  >
                    {new Date(0, 0, 1, minute / 60).toLocaleTimeString(
                      undefined,
                      { hour: "numeric" },
                    )}
                  </span>
                ))}
              </div>
              <div
                className={clsx(
                  "week-day relative min-w-0 flex-1 rounded-lg",
                  !detail.working && "week-day-off",
                )}
              >
                {ticks.slice(1).map((minute) => (
                  <div
                    key={minute}
                    aria-hidden
                    className="week-line absolute inset-x-0"
                    style={{ top: `${(minute / 1440) * 100}%` }}
                  />
                ))}
                {detail.working && (
                  <>
                    <div
                      aria-hidden
                      className="week-off absolute inset-x-0 top-0"
                      style={{ height: `${(hours.startMin / 1440) * 100}%` }}
                    />
                    <div
                      aria-hidden
                      className="week-off absolute inset-x-0 bottom-0"
                      style={{ height: `${100 - (hours.endMin / 1440) * 100}%` }}
                    />
                  </>
                )}
                {detail.blocks.map((block, i) =>
                  block.kind === "busy" ? (
                    <div
                      key={`busy-${block.start}-${i}`}
                      className="week-busy absolute inset-x-1 rounded-[5px] px-1.5"
                      style={{
                        top: `${block.topPct}%`,
                        height: `${block.heightPct}%`,
                      }}
                      title={`Busy · ${range(block)}`}
                    >
                      {block.heightPct >= 2 && (
                        <span className="block truncate pt-0.5 text-[10px] text-[var(--color-ink-muted)]">
                          {range(block)}
                        </span>
                      )}
                    </div>
                  ) : (
                    <div
                      key={`${block.kind}-${block.taskId}-${i}`}
                      data-zone="schedule"
                      className={clsx(
                        "week-task absolute inset-x-1 rounded-[5px] px-1.5",
                        block.kind === "echo" && "week-echo",
                      )}
                      style={{
                        top: `${block.topPct}%`,
                        height: `${block.heightPct}%`,
                      }}
                      title={
                        block.kind === "echo"
                          ? `${block.title} · repeats here · ${range(block)}`
                          : `${block.title} · ${range(block)}`
                      }
                    >
                      <span className="block truncate pt-0.5 text-[10px] text-[var(--zone-ink-muted)]">
                        {block.title}
                        {block.kind === "echo" ? " ↻" : ""}
                      </span>
                    </div>
                  ),
                )}
                {detail.nowPct !== null && (
                  <div
                    aria-hidden
                    className="week-now absolute inset-x-0"
                    style={{ top: `${detail.nowPct}%` }}
                  />
                )}
              </div>
              </div>
            </div>

            {/* The answer: where the room is, then what is booked. */}
            <div className="flex min-w-0 flex-[3] flex-col gap-4 overflow-y-auto">
              <section>
                <h3 className="pb-1.5 text-[10px] tracking-[0.14em] uppercase text-[var(--color-ink-muted)]">
                  Room{isToday ? " left today" : ""}
                </h3>
                {detail.gaps.length === 0 ? (
                  <p className="text-xs text-[var(--color-ink-muted)]">
                    {detail.working
                      ? isToday
                        ? "No room left today."
                        : "The day is fully booked."
                      : "Outside working hours."}
                  </p>
                ) : (
                  <ul className="flex flex-col gap-1">
                    {detail.gaps.map((gap) => (
                      <li
                        key={gap.start}
                        className="flex items-baseline justify-between gap-3 rounded-lg bg-[var(--color-fill-subtle)] px-2.5 py-1.5 text-xs"
                      >
                        <span className="tabular-nums text-[var(--color-ink)]">
                          {range(gap)}
                        </span>
                        <span className="shrink-0 text-[var(--color-ink-muted)]">
                          {formatDuration(gap.end - gap.start)}
                        </span>
                      </li>
                    ))}
                  </ul>
                )}
              </section>

              <section>
                <h3 className="pb-1.5 text-[10px] tracking-[0.14em] uppercase text-[var(--color-ink-muted)]">
                  Booked
                </h3>
                {booked.length === 0 ? (
                  <p className="text-xs text-[var(--color-ink-muted)]">
                    Nothing booked.
                  </p>
                ) : (
                  <ul className="flex flex-col gap-1">
                    {booked.map((block, i) => (
                      <li
                        key={`${block.kind}-${block.start}-${i}`}
                        className="flex items-baseline justify-between gap-3 px-2.5 py-1 text-xs"
                      >
                        <span className="tabular-nums text-[var(--color-ink-muted)]">
                          {range(block)}
                        </span>
                        <span
                          className={clsx(
                            "min-w-0 truncate",
                            block.kind === "task"
                              ? "text-[var(--color-ink)]"
                              : "text-[var(--color-ink-muted)]",
                          )}
                        >
                          {block.kind === "busy"
                            ? "Busy"
                            : block.kind === "echo"
                              ? `${block.title} ↻`
                              : block.title}
                        </span>
                      </li>
                    ))}
                  </ul>
                )}
              </section>
            </div>
          </div>
        </div>
      </div>
    </div>,
    document.body,
  );
}
