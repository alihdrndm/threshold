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
  className,
}: {
  quadrant: Quadrant;
  tasks: Task[];
  onToggleDone: (task: Task) => void;
  onFocus?: (task: Task) => void;
  className?: string;
}) {
  const { setNodeRef, isOver } = useDroppable({ id: quadrant.id });
  const overCap =
    quadrant.id === "do-first" &&
    tasks.filter((t) => t.status === "open").length > DO_FIRST_SOFT_CAP;

  return (
    <section
      ref={setNodeRef}
      className={clsx(
        "flex min-h-40 flex-col gap-2 rounded-2xl border p-4 transition-colors duration-150",
        isOver
          ? "border-[var(--color-accent)] bg-[var(--color-accent)]/[0.06]"
          : "border-[var(--color-border-subtle)] bg-white/[0.015]",
        className,
      )}
    >
      <header className="flex items-baseline justify-between gap-2">
        <h3 className="text-sm font-medium text-[var(--color-ink)]">
          {quadrant.label}
        </h3>
        <span className="text-xs text-[var(--color-ink-muted)]">
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
            />
          ))}
        </div>
      </SortableContext>

      {/* Calm, inline, and not a warning. Four "do first" tasks is a real
          problem, but red text about it would just teach you to ignore red. */}
      {overCap && (
        <p className="mt-1 text-xs text-[var(--color-ink-muted)]">
          Four things cannot all be first. Move one?
        </p>
      )}

      {tasks.length === 0 && (
        <p className="mt-auto text-xs text-[var(--color-ink-muted)]/60">
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
}: {
  tasks: Task[];
  onToggleDone: (task: Task) => void;
  onFocus: (task: Task) => void;
}) {
  const inQuadrant = (q: Quadrant) =>
    tasks.filter((t) =>
      q.urgent === null
        ? t.urgent === null || t.important === null
        : t.urgent === q.urgent && t.important === q.important,
    );

  return (
    <div className="flex gap-4">
      <Zone
        quadrant={INBOX}
        tasks={inQuadrant(INBOX)}
        onToggleDone={onToggleDone}
        onFocus={onFocus}
        className="w-56 shrink-0"
      />
      <div className="grid flex-1 grid-cols-2 gap-4">
        {QUADRANTS.map((quadrant) => (
          <Zone
            key={quadrant.id}
            quadrant={quadrant}
            tasks={inQuadrant(quadrant)}
            onToggleDone={onToggleDone}
            onFocus={onFocus}
          />
        ))}
      </div>
    </div>
  );
}
