import { useDroppable } from "@dnd-kit/core";
import { SortableContext, rectSortingStrategy } from "@dnd-kit/sortable";
import clsx from "clsx";
import type { Task } from "@/lib/tauri";
import { DO_FIRST_SOFT_CAP, INBOX, QUADRANTS, type Quadrant } from "./quadrants";
import { TaskCard } from "./TaskCard";

function Zone({
  quadrant,
  tasks,
  onToggleDone,
  onFocus,
  onDelete,
  activeTaskId,
  sessionRunning,
  className,
}: {
  quadrant: Quadrant;
  tasks: Task[];
  onToggleDone: (task: Task) => void;
  onFocus?: (task: Task) => void;
  onDelete?: (task: Task) => void;
  /** The task a session is running on, if any. */
  activeTaskId?: number | null;
  /** Whether any session is running — this task's or another's. */
  sessionRunning?: boolean;
  className?: string;
}) {
  const { setNodeRef, isOver } = useDroppable({ id: quadrant.id });
  const overCap =
    quadrant.id === "do-first" &&
    tasks.filter((t) => t.status === "open").length > DO_FIRST_SOFT_CAP;

  return (
    // Colour comes from CSS keyed on data-zone, not from a class here. Two
    // background utilities in one string resolve by stylesheet order rather
    // than string order, so a per-quadrant fill and a drag-over fill would
    // fight unpredictably. Custom properties compose through the cascade
    // instead, which is what lets drag-over layer on top of the quadrant's
    // identity rather than replacing it.
    <section
      ref={setNodeRef}
      data-zone={quadrant.id}
      data-over={isOver || undefined}
      className={clsx(
        "matrix-zone flex min-h-40 flex-col gap-2 rounded-2xl border p-4",
        className,
      )}
    >
      {/* The tracked-caps vocabulary the list view already uses for group
          headers, at full ink because a zone is a place, not a caption. The
          hint is sr-only now: sighted readers get the same fact from the axis
          labels around the grid, and saying it twice taught the eye to skip
          both. The count is a mirror, not a meter — it appears only when
          there is something to count. */}
      <header className="flex items-baseline gap-2">
        <h3 className="text-xs font-medium tracking-[0.18em] uppercase text-[var(--color-ink)]">
          {quadrant.label}
          <span className="sr-only"> — {quadrant.hint}</span>
        </h3>
        {tasks.length > 0 && (
          <span className="text-xs text-[var(--zone-ink-muted)] tabular-nums">
            {tasks.length}
          </span>
        )}
      </header>

      <SortableContext items={tasks.map((t) => t.id)} strategy={rectSortingStrategy}>
        <div className="flex flex-col gap-2">
          {tasks.map((task) => (
            <TaskCard
              key={task.id}
              task={task}
              onToggleDone={onToggleDone}
              onFocus={onFocus}
              onDelete={onDelete}
              active={task.id === activeTaskId}
              sessionRunning={sessionRunning}
            />
          ))}
        </div>
      </SortableContext>

      {/* Calm, inline, and not a warning. Four "do first" tasks is a real
          problem, but red text about it would just teach you to ignore red. */}
      {/* Emphasised by contrast rather than by colour — which is the whole
          point of the note above: legible, not loud. */}
      {overCap && (
        <p className="mt-1 text-xs text-[var(--color-ink)]">
          Four things cannot all be first. Move one?
        </p>
      )}

      {/* mt-1, not mt-auto: against a full-height rail the hint would otherwise
          drift far from the header it belongs to. Full strength, because the
          size already does the de-emphasis and 60% opacity fails contrast.
          The copy is the quadrant's own: an empty zone is the one moment its
          meaning is worth a sentence, so the invitation carries it. */}
      {tasks.length === 0 && (
        <p className="mt-1 text-xs text-[var(--zone-ink-muted)]">
          {quadrant.empty}
        </p>
      )}
    </section>
  );
}

/**
 * One word on an axis, quieter and smaller than any zone header so the grid
 * outranks its annotation. aria-hidden because the same fact reaches screen
 * readers through each zone's sr-only hint — position is visual information,
 * and this is its visual form.
 */
function AxisLabel({
  children,
  vertical = false,
}: {
  children: string;
  vertical?: boolean;
}) {
  return (
    <span
      aria-hidden
      className={clsx(
        "place-self-center text-[11px] tracking-[0.22em] uppercase text-[var(--color-ink-muted)] select-none",
        // vertical-rl then flipped, so the left axis reads bottom-to-top the
        // way a y-axis caption does on any chart.
        vertical && "rotate-180 [writing-mode:vertical-rl]",
      )}
    >
      {children}
    </span>
  );
}

export function MatrixView({
  tasks,
  onToggleDone,
  onFocus,
  onDelete,
  activeTaskId,
  sessionRunning,
}: {
  tasks: Task[];
  onToggleDone: (task: Task) => void;
  onFocus: (task: Task) => void;
  onDelete: (task: Task) => void;
  activeTaskId: number | null;
  sessionRunning: boolean;
}) {
  // Completed tasks leave the grid entirely (spec F6). Leaving them in place
  // with only a strikethrough made ticking the box look like nothing happened.
  const open = tasks.filter((t) => t.status !== "done");
  const done = tasks.filter((t) => t.status === "done");

  const inQuadrant = (q: Quadrant) =>
    open.filter((t) =>
      q.urgent === null
        ? t.urgent === null || t.important === null
        : t.urgent === q.urgent && t.important === q.important,
    );

  return (
    <div className="flex flex-col gap-4">
      {/* The axes are the restyle. A 2x2 whose position carries the meaning
          should say so on the axes, once, instead of repeating it inside every
          header — Urgent/Not urgent across the top, Important/Not important
          down the side, and the quadrant hints retire to sr-only. QUADRANTS is
          already laid out in axis order (urgent column first, important row
          first), which is what lets the labels be true.

          One grid for the whole board, not a rail beside a grid: the Inbox
          spans the two quadrant rows so its top edge aligns with Do First
          rather than with the label row — a tray sits beside the matrix, it
          does not outrank its axes. 18rem for the rail: with a checkbox, a
          Focus button and a delete control in a row, 256px left a title
          roughly twelve characters before wrapping. The equal 1fr tracks keep
          the 2x2 a true 2x2 — with solid fills, rows of different heights
          read as a broken layout rather than as content. */}
      <div className="grid grid-cols-[18rem_auto_minmax(0,1fr)_minmax(0,1fr)] grid-rows-[auto_minmax(0,1fr)_minmax(0,1fr)] gap-3">
        <span />
        <span />
        <AxisLabel>Urgent</AxisLabel>
        <AxisLabel>Not urgent</AxisLabel>
        <Zone
          quadrant={INBOX}
          tasks={inQuadrant(INBOX)}
          onToggleDone={onToggleDone}
          onFocus={onFocus}
          onDelete={onDelete}
          activeTaskId={activeTaskId}
          sessionRunning={sessionRunning}
          className="row-span-2"
        />
        <AxisLabel vertical>Important</AxisLabel>
        {QUADRANTS.slice(0, 2).map((quadrant) => (
          <Zone
            key={quadrant.id}
            quadrant={quadrant}
            tasks={inQuadrant(quadrant)}
            onToggleDone={onToggleDone}
            onFocus={onFocus}
            onDelete={onDelete}
            activeTaskId={activeTaskId}
            sessionRunning={sessionRunning}
          />
        ))}
        <AxisLabel vertical>Not important</AxisLabel>
        {QUADRANTS.slice(2).map((quadrant) => (
          <Zone
            key={quadrant.id}
            quadrant={quadrant}
            tasks={inQuadrant(quadrant)}
            onToggleDone={onToggleDone}
            onFocus={onFocus}
            onDelete={onDelete}
            activeTaskId={activeTaskId}
            sessionRunning={sessionRunning}
          />
        ))}
      </div>

      <DoneToday tasks={done} onToggleDone={onToggleDone} onDelete={onDelete} />
    </div>
  );
}

/**
 * A checkmark is the reward. No confetti, no "Amazing!" — celebrating is what
 * licenses the scroll that follows.
 */
export function DoneToday({
  tasks,
  onToggleDone,
  onDelete,
}: {
  tasks: Task[];
  onToggleDone: (task: Task) => void;
  onDelete?: (task: Task) => void;
}) {
  if (tasks.length === 0) return null;

  return (
    // The same machinery as a quadrant, with the quietest fill on the page.
    // A bare transparent box beneath five coloured ones reads as unfinished,
    // but finished work should not be rewarded with emphasis either.
    <section
      data-zone="done"
      className="matrix-zone flex flex-col gap-2 rounded-2xl border p-4"
    >
      {/* Same tracked-caps vocabulary as the zones, muted ink: finished work
          keeps its place in the system without asking for its attention. */}
      <h3 className="text-xs font-medium tracking-[0.18em] uppercase text-[var(--zone-ink-muted)]">
        Done today · {tasks.length}
      </h3>
      <div className="flex flex-col gap-2">
        {tasks.map((task) => (
          <TaskCard
            key={task.id}
            task={task}
            onToggleDone={onToggleDone}
            onDelete={onDelete}
          />
        ))}
      </div>
    </section>
  );
}
