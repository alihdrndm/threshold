import { useEffect, useMemo, useState } from "react";
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
  setTaskStatus,
  type Task,
  type TaskContext,
} from "@/lib/tauri";
import { MatrixView } from "./MatrixView";
import { TaskCard } from "./TaskCard";
import { quadrantById, type QuadrantId } from "./quadrants";

type View = "list" | "matrix";

export function TasksView() {
  const [tasks, setTasks] = useState<Task[]>([]);
  const [contexts, setContexts] = useState<TaskContext[]>([]);
  const [view, setView] = useState<View>("matrix");
  const [contextFilter, setContextFilter] = useState<number | null>(null);
  const [draft, setDraft] = useState("");
  const [dragging, setDragging] = useState<Task | null>(null);

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

  async function submitDraft() {
    const title = draft.trim();
    if (!title) return;
    setDraft("");
    await addTask(title, contextFilter);
    await refresh();
  }

  async function toggleDone(task: Task) {
    await setTaskStatus(task.id, task.status === "done" ? "open" : "done");
    await refresh();
  }

  async function focus(task: Task) {
    await setTaskStatus(task.id, task.status);
    await focusOnTask();
  }

  function onDragStart(event: DragStartEvent) {
    setDragging(tasks.find((t) => t.id === event.active.id) ?? null);
  }

  async function onDragEnd(event: DragEndEvent) {
    setDragging(null);
    const { active, over } = event;
    if (!over) return;

    // Dropping onto a zone gives the quadrant id; dropping onto another card
    // means "the quadrant that card is in".
    const overId = String(over.id);
    const target = tasks.find((t) => String(t.id) === overId);
    const quadrantId = (target
      ? quadrantOfTask(target)
      : overId) as QuadrantId;

    const quadrant = quadrantById(quadrantId);
    const moved = tasks.find((t) => t.id === active.id);
    if (!moved) return;
    if (moved.urgent === quadrant.urgent && moved.important === quadrant.important)
      return;

    // Optimistic: the drop should feel instantaneous, not wait on SQLite.
    setTasks((current) =>
      current.map((t) =>
        t.id === moved.id
          ? { ...t, urgent: quadrant.urgent, important: quadrant.important }
          : t,
      ),
    );
    await moveTask(moved.id, quadrant.urgent, quadrant.important, 0);
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
          if (event.key === "Enter") void submitDraft();
        }}
        placeholder="Add a task"
        className="ritual-field w-full rounded-full border border-[var(--color-border-subtle)] bg-white/[0.03] px-5 py-3 text-sm text-[var(--color-ink)] outline-none placeholder:text-[var(--color-ink-muted)]/60 focus:border-[color-mix(in_srgb,var(--color-accent)_60%,transparent)]"
      />

      <DndContext
        sensors={sensors}
        collisionDetection={closestCorners}
        onDragStart={onDragStart}
        onDragEnd={onDragEnd}
      >
        <div className="min-h-0 flex-1 overflow-auto">
          {view === "matrix" ? (
            <MatrixView
              tasks={visible}
              onToggleDone={(t) => void toggleDone(t)}
              onFocus={(t) => void focus(t)}
            />
          ) : (
            <ListView
              tasks={visible}
              contexts={contexts}
              onToggleDone={(t) => void toggleDone(t)}
              onFocus={(t) => void focus(t)}
            />
          )}
        </div>

        {/* The dragged card follows the cursor at full opacity so the drop
            target underneath stays readable. */}
        <DragOverlay>
          {dragging && (
            <div className="rounded-xl border border-[var(--color-accent)] bg-[var(--color-surface-raised)] px-3 py-2.5 text-sm shadow-lg">
              {dragging.title}
            </div>
          )}
        </DragOverlay>
      </DndContext>
    </div>
  );
}

function quadrantOfTask(task: Task): QuadrantId {
  if (task.urgent === null || task.important === null) return "inbox";
  if (task.urgent && task.important) return "do-first";
  if (!task.urgent && task.important) return "schedule";
  if (task.urgent && !task.important) return "delegate";
  return "eliminate";
}

function ListView({
  tasks,
  contexts,
  onToggleDone,
  onFocus,
}: {
  tasks: Task[];
  contexts: TaskContext[];
  onToggleDone: (task: Task) => void;
  onFocus: (task: Task) => void;
}) {
  const groups = [
    ...contexts.map((context) => ({
      key: String(context.id),
      name: context.name,
      items: tasks.filter((t) => t.contextId === context.id),
    })),
    {
      key: "none",
      name: "No context",
      items: tasks.filter((t) => t.contextId === null),
    },
  ].filter((group) => group.items.length > 0);

  if (groups.length === 0) {
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
              />
            ))}
          </SortableContext>
        </section>
      ))}
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
              ? "bg-white/10 text-[var(--color-ink)]"
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
