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
      <header className="flex items-baseline justify-between gap-2">
        <h3 className="text-sm font-medium text-[var(--color-ink)]">
          {quadrant.label}
        </h3>
        <span className="text-xs text-[var(--zone-ink-muted)]">
          {quadrant.hint}
        </span>
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
          size already does the de-emphasis and 60% opacity fails contrast. */}
      {tasks.length === 0 && (
        <p className="mt-1 text-xs text-[var(--zone-ink-muted)]">
          Drop a task here
        </p>
      )}
    </section>
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
      <div className="flex gap-4">
        <Zone
          quadrant={INBOX}
          tasks={inQuadrant(INBOX)}
          onToggleDone={onToggleDone}
          onFocus={onFocus}
          onDelete={onDelete}
          activeTaskId={activeTaskId}
          sessionRunning={sessionRunning}
          // w-72, not w-64: with a checkbox, a Focus button and a delete
          // control in the row, 256px left a title roughly twelve characters
          // before wrapping — the rail exists to *hold* unclassified tasks,
          // so it gets the space to show them.
          className="w-72 shrink-0"
        />
        {/* auto-rows-fr keeps the 2x2 a true 2x2: with solid fills, rows of
            different heights read as a broken layout rather than as content. */}
        <div className="grid flex-1 auto-rows-fr grid-cols-2 gap-4">
          {QUADRANTS.map((quadrant) => (
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
      <h3 className="text-sm font-medium text-[var(--zone-ink-muted)]">
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
