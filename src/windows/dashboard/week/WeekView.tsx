import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import clsx from "clsx";
import { listen } from "@tauri-apps/api/event";
import {
  calendarStatus,
  calendarWeek,
  listTasks,
  type CalendarWeek,
  type Task,
} from "@/lib/tauri";
import { useWidth } from "../useWidth";
import { buildWeek, hourTicks, tickLabel, type Axis } from "./week";

/**
 * The coming week as free and busy space - the scheduling-assistant strip.
 *
 * Read-only on purpose: its one job is the judgement "where is there room
 * this week", made without leaving the board. Busy blocks are opaque shapes
 * (Google's free/busy carries no titles, and the judgement needs none);
 * Threshold's own scheduled tasks are the exception - they are local data,
 * so they show their names, drawn over their own footprint in Google's busy.
 *
 * Placement-agnostic: give it `tasks` to share the caller's list, or it
 * fetches its own. It listens for `tasks-changed` like the rest of the
 * dashboard, and the backend's short cache absorbs the toggling.
 */

/** What the grid draws before the first payload lands: the default hours. */
const DEFAULT_HOURS = {
  startMin: 9 * 60,
  endMin: 18 * 60,
  days: [true, true, true, true, true, false, false],
  bufferMin: 15,
};

function todayMidnight(): number {
  const d = new Date();
  d.setHours(0, 0, 0, 0);
  return Math.floor(d.getTime() / 1000);
}

function clockTime(ts: number): string {
  return new Date(ts * 1000).toLocaleTimeString(undefined, {
    hour: "numeric",
    minute: "2-digit",
  });
}

export function WeekView({ tasks }: { tasks?: Task[] }) {
  const [data, setData] = useState<CalendarWeek | null>(null);
  // null = not asked yet; the invite only shows once we know.
  const [connected, setConnected] = useState<boolean | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [ownTasks, setOwnTasks] = useState<Task[]>([]);
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000));
  const lastFocus = useRef(0);
  const [ref, width] = useWidth<HTMLElement>();

  const needOwnTasks = tasks === undefined;

  const refetch = useCallback(async () => {
    try {
      const status = await calendarStatus();
      setConnected(status.connected);
      if (!status.connected) return;
      const [week, list] = await Promise.all([
        calendarWeek(),
        needOwnTasks ? listTasks() : Promise.resolve(null),
      ]);
      setData(week);
      if (list) setOwnTasks(list);
      setError(null);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      // Disconnecting mid-view is an answer, not a failure.
      if (/not connected/i.test(message)) setConnected(false);
      else setError(message);
    }
  }, [needOwnTasks]);

  useEffect(() => {
    void refetch();
    const unlisten = listen("tasks-changed", () => void refetch());
    // Coming back to the window after a while is when the week most likely
    // changed elsewhere; same 30 s throttle the tasks board uses.
    const onFocus = () => {
      if (Date.now() - lastFocus.current < 30_000) return;
      lastFocus.current = Date.now();
      void refetch();
    };
    window.addEventListener("focus", onFocus);
    // Only the now-line moves on its own; no IPC in this timer.
    const tick = window.setInterval(
      () => setNow(Math.floor(Date.now() / 1000)),
      60_000,
    );
    return () => {
      void unlisten.then((off) => off());
      window.removeEventListener("focus", onFocus);
      window.clearInterval(tick);
    };
  }, [refetch]);

  const source = tasks ?? ownTasks;
  const scheduled = useMemo(
    () =>
      source
        .filter((t) => t.status === "open" && t.scheduledTs !== null)
        .map((t) => ({ id: t.id, title: t.title, scheduledTs: t.scheduledTs! })),
    [source],
  );

  const week = useMemo(() => {
    return buildWeek(
      {
        startTs: data?.startTs ?? todayMidnight(),
        busy: (data?.busy ?? []).map((b) => ({ start: b.startTs, end: b.endTs })),
        hours: data?.hours ?? DEFAULT_HOURS,
      },
      scheduled,
      now,
    );
  }, [data, scheduled, now]);

  if (connected === false) {
    return (
      <section aria-label="The coming week" className="week-panel">
        <p className="rounded-2xl border border-[var(--color-border-subtle)] bg-[var(--color-fill-subtle)] px-4 py-5 text-center text-sm text-[var(--color-ink-muted)]">
          Connect Google Calendar in Settings to see your week.
        </p>
      </section>
    );
  }

  const hours = data?.hours ?? DEFAULT_HOURS;
  const narrow = width > 0 && width < 480;
  const ticks = hourTicks(week.axis, width > 0 && width < 720 ? 120 : 60);
  const emptyWeek =
    data !== null && data.busy.length === 0 && scheduled.length === 0;

  return (
    <section ref={ref} aria-label="The coming week" className="week-panel flex flex-col gap-2">
      {error && (
        <p className="rounded-xl border border-[var(--color-accent)]/50 bg-[var(--color-accent)]/[0.08] px-4 py-2.5 text-sm text-[var(--color-ink)]">
          {error}
        </p>
      )}

      <div className={clsx("min-w-0", narrow && "overflow-x-auto pb-1")}>
        <div
          className="grid gap-x-1"
          style={{
            gridTemplateColumns: "max-content repeat(7, minmax(0, 1fr))",
            minWidth: narrow ? 560 : undefined,
          }}
        >
          {/* Day headers. Today is named, the rest carry their date. */}
          <div />
          {week.days.map((day) => {
            const d = new Date(day.dayStart * 1000);
            const isToday =
              d.toDateString() === new Date(now * 1000).toDateString();
            return (
              <div
                key={day.dayStart}
                className={clsx(
                  "pb-1.5 text-center text-[10px] tracking-[0.14em] uppercase",
                  isToday
                    ? "text-[var(--color-ink)]"
                    : "text-[var(--color-ink-muted)]",
                )}
              >
                {isToday
                  ? "Today"
                  : d.toLocaleDateString(undefined, { weekday: "short" })}
                <span className="ml-1 opacity-60 tabular-nums">{d.getDate()}</span>
              </div>
            );
          })}

          {/* The hour axis. */}
          <div className="relative h-52 w-10" aria-hidden>
            {ticks.map((minute) => (
              <span
                key={minute}
                className="absolute right-1.5 -translate-y-1/2 text-[9px] tabular-nums whitespace-nowrap text-[var(--color-ink-muted)]"
                style={{ top: `${toPct(minute, week.axis)}%` }}
              >
                {tickLabel(minute)}
              </span>
            ))}
          </div>

          {/* Seven columns of the week itself. */}
          {week.days.map((day) => (
            <div
              key={day.dayStart}
              className={clsx(
                "week-day relative h-52 overflow-hidden rounded-lg",
                !day.working && "week-day-off",
              )}
            >
              {ticks.map((minute) => (
                <div
                  key={minute}
                  aria-hidden
                  className="week-line absolute inset-x-0"
                  style={{ top: `${toPct(minute, week.axis)}%` }}
                />
              ))}
              {/* The night, shaded: before the day opens and after it closes. */}
              {day.working && (
                <>
                  <div
                    aria-hidden
                    className="week-off absolute inset-x-0 top-0"
                    style={{ height: `${toPct(hours.startMin, week.axis)}%` }}
                  />
                  <div
                    aria-hidden
                    className="week-off absolute inset-x-0 bottom-0"
                    style={{ height: `${100 - toPct(hours.endMin, week.axis)}%` }}
                  />
                </>
              )}
              {day.blocks.map((block, i) =>
                block.kind === "busy" ? (
                  <div
                    key={`busy-${block.start}-${i}`}
                    className="week-block week-busy absolute inset-x-0.5 rounded-[5px]"
                    style={blockStyle(block.topPct, block.heightPct, i)}
                    title={`Busy · ${clockTime(block.start)} – ${clockTime(block.end)}`}
                  />
                ) : (
                  <div
                    key={`task-${block.taskId}-${i}`}
                    data-zone="schedule"
                    className="week-block week-task absolute inset-x-0.5 rounded-[5px] px-1"
                    style={blockStyle(block.topPct, block.heightPct, i)}
                    title={`${block.title} · ${clockTime(block.start)} – ${clockTime(block.end)}`}
                  >
                    <span className="block truncate text-[9px] leading-[14px] text-[var(--zone-ink-muted)]">
                      {block.title}
                    </span>
                  </div>
                ),
              )}
              {day.nowPct !== null && (
                <div
                  aria-hidden
                  className="week-now absolute inset-x-0"
                  style={{ top: `${day.nowPct}%` }}
                />
              )}
            </div>
          ))}
        </div>
      </div>

      {/* The key, and the good news when there is nothing to draw. */}
      <p className="flex flex-wrap items-center gap-x-4 gap-y-1 px-1 text-[10px] tracking-[0.14em] uppercase text-[var(--color-ink-muted)]">
        <span className="flex items-center gap-1.5">
          <span aria-hidden className="week-key week-busy" /> busy
        </span>
        <span className="flex items-center gap-1.5">
          <span aria-hidden data-zone="schedule" className="week-key week-task" />{" "}
          your tasks
        </span>
        {emptyWeek && (
          <span className="normal-case tracking-normal">
            Nothing booked in the coming week.
          </span>
        )}
      </p>
    </section>
  );
}

/** Minutes past midnight to a percentage of the axis, clamped to the grid. */
function toPct(minute: number, axis: Axis): number {
  const span = axis.endMin - axis.startMin;
  if (span <= 0) return 0;
  return Math.min(100, Math.max(0, ((minute - axis.startMin) / span) * 100));
}

/** A block's position plus its stagger index for the entrance animation. */
function blockStyle(
  topPct: number,
  heightPct: number,
  index: number,
): React.CSSProperties {
  return {
    top: `${topPct}%`,
    height: `${heightPct}%`,
    ["--i" as never]: String(index),
  };
}
