import { useEffect, useMemo, useRef, useState } from "react";
import {
  DndContext,
  DragOverlay,
  KeyboardSensor,
  PointerSensor,
  closestCorners,
  useSensor,
  useSensors,
  type DragEndEvent,
  type DragStartEvent,
} from "@dnd-kit/core";
import {
  SortableContext,
  sortableKeyboardCoordinates,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import clsx from "clsx";
import {
  addTask,
  focusOnTask,
  listContexts,
  listTasks,
  moveTask,
  reorderTasks,
  setTaskStatus,
  type Task,
  type TaskContext,
} from "@/lib/tauri";
import { DoneToday, MatrixView } from "./MatrixView";
import { TaskCard } from "./TaskCard";
import {
  inQuadrant,
  orderAfterDrop,
  quadrantById,
  quadrantOf,
  type QuadrantId,
} from "./quadrants";

type View = "list" | "matrix";

export function TasksView({
  activeTaskId = null,
  sessionRunning = false,
}: {
  activeTaskId?: number | null;
  sessionRunning?: boolean;
} = {}) {
  const [tasks, setTasks] = useState<Task[]>([]);
  const [contexts, setContexts] = useState<TaskContext[]>([]);
  const [view, setView] = useState<View>("matrix");
  const [contextFilter, setContextFilter] = useState<number | null>(null);
  const [draft, setDraft] = useState("");
  const [dragging, setDragging] = useState<Task | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<{ text: string; undo: () => void } | null>(
    null,
  );
  const noticeTimer = useRef<number | null>(null);

  useEffect(
    () => () => {
      if (noticeTimer.current !== null) window.clearTimeout(noticeTimer.current);
    },
    [],
  );

  // Pointer needs a small activation distance, or a click on the done checkbox
  // registers as a micro-drag and never fires. Keyboard is not an afterthought:
  // reclassifying tasks is the primary interaction here, and an interaction you
  // can only perform with a mouse is one some people simply cannot perform.
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 4 } }),
    useSensor(KeyboardSensor, {
      coordinateGetter: sortableKeyboardCoordinates,
    }),
  );

  async function refresh() {
    const [t, c] = await Promise.all([listTasks(), listContexts()]);
    setTasks(t);
    setContexts(c);
  }

  useEffect(() => {
    void refresh();
  }, []);

  const visible = useMemo(
    () =>
      contextFilter === null
        ? tasks
        : tasks.filter((t) => t.contextId === contextFilter),
    [tasks, contextFilter],
  );

  /// Every action reports its own failure. Previously each was fired with
  /// `void`, so a rejected invoke vanished with no console line and no visible
  /// change — which is what "the checkbox does nothing" actually looked like.
  function run(action: () => Promise<void>) {
    setError(null);
    action().catch((err: unknown) =>
      setError(err instanceof Error ? err.message : String(err)),
    );
  }

  async function submitDraft() {
    const title = draft.trim();
    if (!title) return;
    setDraft("");
    await addTask(title, contextFilter);
    await refresh();
  }

  /// Deleting asks nothing and forgives everything: the row keeps its quadrant
  /// and its status in the database, so Undo is a plain restore rather than a
  /// reconstruction. A confirm dialog guards against a click; an undo forgives
  /// one, and costs nothing on the nineteen deletes that were meant.
  async function deleteTask(task: Task) {
    // Optimistic: the card leaves now, not after SQLite.
    setTasks((current) => current.filter((t) => t.id !== task.id));
    await setTaskStatus(task.id, "deleted");
    const before = task.status;
    if (noticeTimer.current !== null) window.clearTimeout(noticeTimer.current);
    setNotice({
      text: `Deleted “${task.title}”`,
      undo: () =>
        run(async () => {
          setNotice(null);
          await setTaskStatus(task.id, before);
          await refresh();
        }),
    });
    noticeTimer.current = window.setTimeout(() => setNotice(null), 6000);
  }

  async function toggleDone(task: Task) {
    const next = task.status === "done" ? "open" : "done";
    // Optimistic: ticking a box should feel instant, not wait on SQLite.
    setTasks((current) =>
      current.map((t) => (t.id === task.id ? { ...t, status: next } : t)),
    );
    await setTaskStatus(task.id, next);
    await refresh();
  }

  async function focus(task: Task) {
    // Answered here rather than by disabling the button, so the refusal says
    // something. Rust refuses too - this is not the guard, only the faster one.
    if (sessionRunning && task.id !== activeTaskId) {
      setError(
        "A session is already running. End it from the banner above to start another.",
      );
      return;
    }
    const result = await focusOnTask(task.id);
    if (!result.opened) {
      setError(result.reason ?? "The ritual could not be opened.");
    }
  }

  function onDragStart(event: DragStartEvent) {
    setDragging(tasks.find((t) => t.id === event.active.id) ?? null);
  }

  async function onDragEnd(event: DragEndEvent) {
    setDragging(null);
    const { active, over } = event;
    if (!over || active.id === over.id) return;

    // Dropping onto a zone gives the quadrant id; dropping onto another card
    // means "the quadrant that card is in".
    const overId = String(over.id);
    const target = tasks.find((t) => String(t.id) === overId);
    const quadrantId = (target ? quadrantOf(target) : overId) as QuadrantId;

    const quadrant = quadrantById(quadrantId);
    const moved = tasks.find((t) => t.id === active.id);
    if (!moved) return;

    // The zone's full membership from `tasks`, not `visible`: reordering under
    // a context filter must not scramble the tasks the filter is hiding.
    const zone = tasks.filter(
      (t) => t.status !== "done" && inQuadrant(t, quadrant),
    );
    const overIndex = target ? zone.findIndex((t) => t.id === target.id) : -1;
    const ordered = orderAfterDrop(zone, moved, overIndex);
    if (!ordered) return;

    const position = ordered.findIndex((t) => t.id === moved.id);
    const orderOf = new Map(ordered.map((t, index) => [t.id, index]));
    const crossedZones =
      moved.urgent !== quadrant.urgent || moved.important !== quadrant.important;

    // Optimistic: flags on the moved card, fresh indices across the target
    // zone, then the same (sortOrder, id) sort the backend reads with - so
    // the card settles where it will land, not after SQLite says so.
    setTasks((current) =>
      current
        .map((t) => {
          const next =
            t.id === moved.id
              ? { ...t, urgent: quadrant.urgent, important: quadrant.important }
              : t;
          const order = orderOf.get(t.id);
          return order === undefined ? next : { ...next, sortOrder: order };
        })
        .sort((a, b) => a.sortOrder - b.sortOrder || a.id - b.id),
    );

    if (crossedZones) {
      await moveTask(moved.id, quadrant.urgent, quadrant.important, position);
    }
    await reorderTasks(ordered.map((t) => t.id));
    await refresh();
  }

  return (
    <div className="flex h-full flex-col gap-5 p-8">
      <header className="flex flex-wrap items-center gap-3">
        <Segmented value={view} onChange={setView} />
        <div className="ml-auto flex flex-wrap gap-2">
          <Chip
            active={contextFilter === null}
            onClick={() => setContextFilter(null)}
          >
            All
          </Chip>
          {contexts.map((context) => (
            <Chip
              key={context.id}
              active={contextFilter === context.id}
              onClick={() => setContextFilter(context.id)}
            >
              {context.name}
            </Chip>
          ))}
        </div>
      </header>

      <input
        value={draft}
        onChange={(event) => setDraft(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter") run(() => submitDraft());
        }}
        placeholder="Add a task"
        className="ritual-field w-full rounded-full border border-[var(--color-border-subtle)] bg-[var(--color-fill-subtle)] px-5 py-3 text-sm text-[var(--color-ink)] outline-none placeholder:text-[var(--color-ink-muted)] focus:border-[color-mix(in_srgb,var(--color-accent)_60%,transparent)]"
      />

      {error && (
        <p className="rounded-xl border border-[var(--color-accent)]/50 bg-[var(--color-accent)]/[0.08] px-4 py-2.5 text-sm text-[var(--color-ink)]">
          {error}
        </p>
      )}

      {/* Quieter than the error line on purpose: this reports something that
          went right. Undo is styled like Focus — the interface keeps one
          vocabulary for "small round thing you can press". */}
      {notice && (
        <div className="matrix-notice flex items-center gap-3 rounded-xl border border-[var(--color-border-subtle)] bg-[var(--color-fill-subtle)] px-4 py-2 text-sm">
          <span className="min-w-0 flex-1 truncate text-[var(--color-ink-muted)]">
            {notice.text}
          </span>
          <button
            type="button"
            onClick={notice.undo}
            className="ritual-pressable shrink-0 rounded-full border border-[color-mix(in_srgb,var(--color-ink)_16%,transparent)] px-2.5 py-1 text-xs text-[var(--color-ink)]"
          >
            Undo
          </button>
        </div>
      )}

      <DndContext
        sensors={sensors}
        collisionDetection={closestCorners}
        onDragStart={onDragStart}
        onDragCancel={() => setDragging(null)}
        onDragEnd={onDragEnd}
      >
        <div className="min-h-0 flex-1 overflow-auto">
          {view === "matrix" ? (
            <MatrixView
              tasks={visible}
              onToggleDone={(t) => run(() => toggleDone(t))}
              onFocus={(t) => run(() => focus(t))}
              onDelete={(t) => run(() => deleteTask(t))}
              activeTaskId={activeTaskId}
              sessionRunning={sessionRunning}
            />
          ) : (
            <ListView
              tasks={visible}
              contexts={contexts}
              onToggleDone={(t) => run(() => toggleDone(t))}
              onFocus={(t) => run(() => focus(t))}
              onDelete={(t) => run(() => deleteTask(t))}
            />
          )}
        </div>

        {/* The dragged card follows the cursor at full opacity so the drop
            target underneath stays readable. */}
        <DragOverlay>
          {dragging && (
            <div className="matrix-drag-overlay rounded-xl px-3 py-2.5 text-sm">
              {dragging.title}
            </div>
          )}
        </DragOverlay>
      </DndContext>
    </div>
  );
}

function ListView({
  tasks,
  contexts,
  onToggleDone,
  onFocus,
  onDelete,
}: {
  tasks: Task[];
  contexts: TaskContext[];
  onToggleDone: (task: Task) => void;
  onFocus: (task: Task) => void;
  onDelete: (task: Task) => void;
}) {
  const open = tasks.filter((t) => t.status !== "done");
  const done = tasks.filter((t) => t.status === "done");

  const groups = [
    ...contexts.map((context) => ({
      key: String(context.id),
      name: context.name,
      items: open.filter((t) => t.contextId === context.id),
    })),
    {
      key: "none",
      name: "No context",
      items: open.filter((t) => t.contextId === null),
    },
  ].filter((group) => group.items.length > 0);

  if (groups.length === 0 && done.length === 0) {
    return (
      <p className="text-sm text-[var(--color-ink-muted)]">
        Nothing on the list yet.
      </p>
    );
  }

  return (
    <div className="flex flex-col gap-6">
      {groups.map((group) => (
        <section key={group.key} className="flex flex-col gap-2">
          <h3 className="text-xs tracking-[0.18em] text-[var(--color-ink-muted)] uppercase">
            {group.name}
          </h3>
          <SortableContext
            items={group.items.map((t) => t.id)}
            strategy={verticalListSortingStrategy}
          >
            {group.items.map((task) => (
              <TaskCard
                key={task.id}
                task={task}
                onToggleDone={onToggleDone}
                onFocus={onFocus}
                onDelete={onDelete}
              />
            ))}
          </SortableContext>
        </section>
      ))}
      <DoneToday tasks={done} onToggleDone={onToggleDone} onDelete={onDelete} />
    </div>
  );
}

function Segmented({
  value,
  onChange,
}: {
  value: View;
  onChange: (view: View) => void;
}) {
  return (
    <div className="flex gap-1 rounded-full border border-[var(--color-border-subtle)] p-1">
      {(["matrix", "list"] as const).map((option) => (
        <button
          key={option}
          type="button"
          onClick={() => onChange(option)}
          className={clsx(
            "rounded-full px-4 py-1.5 text-sm capitalize transition-colors duration-150",
            value === option
              ? "bg-[var(--color-fill-selected)] text-[var(--color-ink)]"
              : "text-[var(--color-ink-muted)] hover:text-[var(--color-ink)]",
          )}
        >
          {option}
        </button>
      ))}
    </div>
  );
}

function Chip({
  children,
  active,
  onClick,
}: {
  children: React.ReactNode;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={clsx(
        "rounded-full border px-3 py-1.5 text-xs transition-colors duration-150",
        active
          ? "border-[var(--color-accent)] text-[var(--color-ink)]"
          : "border-[var(--color-border-subtle)] text-[var(--color-ink-muted)] hover:text-[var(--color-ink)]",
      )}
    >
      {children}
    </button>
  );
}
