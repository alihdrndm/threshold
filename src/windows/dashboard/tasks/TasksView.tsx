import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  DndContext,
  DragOverlay,
  KeyboardSensor,
  PointerSensor,
  closestCorners,
  useDroppable,
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
  addContext,
  addTask,
  focusOnTask,
  listContexts,
  listTasks,
  moveTask,
  reorderTasks,
  setTaskContext,
  setTaskStatus,
  type Task,
  type TaskContext,
} from "@/lib/tauri";
import { AreasContext } from "./AreaMenu";
import { Popover } from "./Popover";
import { parseTitle, suggestAreas, tagAtCaret } from "./areas";
import { DoneToday, MatrixView } from "./MatrixView";
import { TaskCard } from "./TaskCard";
import {
  PLACE_ORDER,
  inQuadrant,
  orderAfterDrop,
  placeOf,
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
  const [search, setSearch] = useState("");
  const [dragging, setDragging] = useState<Task | null>(null);
  const [error, setError] = useState<string | null>(null);
  // One quiet line for things that went right, with at most one thing to
  // press: Undo after a delete, Create after an unknown #area.
  const [notice, setNotice] = useState<{
    text: string;
    action: { label: string; run: () => void };
  } | null>(null);
  const noticeTimer = useRef<number | null>(null);
  const draftField = useRef<HTMLInputElement>(null);
  // The `#area` autocomplete: where the caret is, and which suggestion is lit.
  const [caret, setCaret] = useState(0);
  const [lit, setLit] = useState(0);
  const [newArea, setNewArea] = useState<string | null>(null);
  const closeCompletions = useCallback(() => setCaret(0), []);

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

  // Search reaches across everything - every zone, the Done pile, and past any
  // context filter. A search that silently honoured a forgotten filter would
  // answer "it's gone" when the truth is "it's hidden".
  const query = search.trim().toLowerCase();
  const results = useMemo(
    () =>
      query === ""
        ? []
        : tasks
            .filter((t) => t.title.toLowerCase().includes(query))
            .map((task) => ({ task, place: placeOf(task) }))
            // Stable sort: within a zone the board's own order survives.
            .sort(
              (a, b) =>
                PLACE_ORDER.indexOf(a.place.zone) -
                PLACE_ORDER.indexOf(b.place.zone),
            ),
    [tasks, query],
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

  function say(text: string, action: { label: string; run: () => void }) {
    if (noticeTimer.current !== null) window.clearTimeout(noticeTimer.current);
    setNotice({ text, action });
    noticeTimer.current = window.setTimeout(() => setNotice(null), 6000);
  }

  /// A `#area` in the title wins over the selected chip: the tag was typed
  /// for this task, the chip was chosen for the view. A tag naming no area
  /// files the task without one and offers to create the area - a name that
  /// vanished on Enter would be a name you thought was saved.
  async function submitDraft() {
    const parsed = parseTitle(draft, contexts);
    if (!parsed.title) return;
    setDraft("");
    const id = await addTask(parsed.title, parsed.context?.id ?? contextFilter);
    await refresh();
    if (parsed.unknown) {
      const name = parsed.unknown;
      say(`No area called “${name}” yet`, {
        label: `Create ${name}`,
        run: () =>
          run(async () => {
            setNotice(null);
            const made = await makeArea(name);
            if (made) await setTaskContext(id, made.id);
            await refresh();
          }),
      });
    }
  }

  /// Create an area and hand back the row as stored - the name may have been
  /// tidied - or null if it could not be found in what came back.
  async function makeArea(name: string): Promise<TaskContext | null> {
    const next = await addContext(name);
    setContexts(next);
    return (
      next.find((c) => c.name.toLowerCase() === name.trim().toLowerCase()) ?? null
    );
  }

  /// The chosen suggestion replaces the half-typed tag; the caret lands after
  /// it, ready for the rest of the title.
  function completeTag(name: string) {
    const tag = tagAtCaret(draft, caret);
    if (!tag) return;
    const next = `${draft.slice(0, tag.start)}#${name} ${draft.slice(caret).trimStart()}`;
    setDraft(next);
    const at = tag.start + name.length + 2;
    setCaret(at);
    requestAnimationFrame(() => draftField.current?.setSelectionRange(at, at));
  }

  async function setArea(task: Task, contextId: number | null) {
    if (task.contextId === contextId) return;
    setTasks((current) =>
      current.map((t) => (t.id === task.id ? { ...t, contextId } : t)),
    );
    await setTaskContext(task.id, contextId);
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
    say(`Deleted “${task.title}”`, {
      label: "Undo",
      run: () =>
        run(async () => {
          setNotice(null);
          await setTaskStatus(task.id, before);
          await refresh();
        }),
    });
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

    const overId = String(over.id);

    // Dropped on an area chip: the task changes home, not place. The chips
    // are the areas, so the board's drag vocabulary reaches them for free.
    if (overId.startsWith("area:")) {
      const raw = overId.slice("area:".length);
      const dropped = tasks.find((t) => t.id === active.id);
      if (dropped) await setArea(dropped, raw === "none" ? null : Number(raw));
      return;
    }

    // Dropping onto a zone gives the quadrant id; dropping onto another card
    // means "the quadrant that card is in".
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

  const filterName = contexts.find((c) => c.id === contextFilter)?.name;
  const tag = tagAtCaret(draft, caret);
  const suggestions = tag ? suggestAreas(tag.query, contexts) : [];
  // What the popover offers: matching areas, or - when nothing matches and
  // something was typed - one item that makes the area on the spot.
  const canMake =
    tag !== null &&
    tag.query.length > 0 &&
    !contexts.some((c) => c.name.toLowerCase() === tag.query.toLowerCase());
  const options = suggestions.length + (canMake ? 1 : 0);
  const showPopover = tag !== null && options > 0;

  /// Accept whatever the popover has lit: an area, or the offer to make one.
  function acceptLit(index = lit) {
    const pick = suggestions[index];
    if (pick) {
      completeTag(pick.name);
      return;
    }
    if (canMake && tag) {
      const query = tag.query;
      run(async () => {
        const made = await makeArea(query);
        if (made) completeTag(made.name);
      });
    }
  }

  return (
    <AreasContext.Provider
      value={{ contexts, setArea: (t, id) => run(() => setArea(t, id)) }}
    >
    <div className="flex h-full flex-col gap-5 p-8">
      {/* The drag layer wraps the chips as well as the board: they take
          cards now. */}
      <DndContext
        sensors={sensors}
        collisionDetection={closestCorners}
        onDragStart={onDragStart}
        onDragCancel={() => setDragging(null)}
        onDragEnd={onDragEnd}
      >
      <header className="flex flex-wrap items-center gap-3">
        <Segmented value={view} onChange={setView} />
        {/* The areas. Each chip filters when clicked and takes a card when
            one is dropped on it: the same word - Job - is both the lens and
            the label, so there is one thing to learn. */}
        <div className="ml-auto flex flex-wrap items-center gap-2">
          <Chip
            dropId="area:none"
            active={contextFilter === null}
            onClick={() => setContextFilter(null)}
          >
            All
          </Chip>
          {contexts.map((context) => (
            <Chip
              key={context.id}
              dropId={`area:${context.id}`}
              active={contextFilter === context.id}
              onClick={() => setContextFilter(context.id)}
            >
              {context.name}
            </Chip>
          ))}
          {/* One more, where the areas are. Rename and remove live in
              Settings: here you are looking at tasks, not administering
              labels, and the one gesture that belongs here is "another". */}
          {newArea === null ? (
            <button
              type="button"
              onClick={() => setNewArea("")}
              aria-label="New area"
              className="ritual-pressable grid size-7 place-items-center rounded-full border border-[var(--color-border-subtle)] text-sm text-[var(--color-ink-muted)] hover:text-[var(--color-ink)]"
            >
              +
            </button>
          ) : (
            <input
              autoFocus
              value={newArea}
              onChange={(event) => setNewArea(event.target.value)}
              onBlur={() => {
                if (!newArea.trim()) setNewArea(null);
              }}
              onKeyDown={(event) => {
                if (event.key === "Enter" && newArea.trim())
                  run(async () => {
                    await makeArea(newArea);
                    setNewArea(null);
                  });
                if (event.key === "Escape") setNewArea(null);
              }}
              placeholder="New area"
              aria-label="New area name"
              className="ritual-field w-32 rounded-full border border-[color-mix(in_srgb,var(--color-accent)_60%,transparent)] bg-[var(--color-fill-subtle)] px-3 py-1.5 text-xs text-[var(--color-ink)] outline-none placeholder:text-[var(--color-ink-muted)]"
            />
          )}
        </div>
      </header>

      {/* Two fields, two verbs: the wide one adds, the narrow one finds. One
          field doing both would need a mode, and a mode needs explaining. */}
      {/* Wraps below ~40rem: the search drops under the add field rather
          than squeezing it into a slot a title cannot fit. */}
      <div className="flex flex-wrap gap-3">
        <div className="relative min-w-64 flex-[1_1_20rem]">
        <input
          ref={draftField}
          value={draft}
          onChange={(event) => {
            setDraft(event.target.value);
            setCaret(event.target.selectionStart ?? event.target.value.length);
            setLit(0);
          }}
          onSelect={(event) =>
            setCaret(event.currentTarget.selectionStart ?? draft.length)
          }
          onKeyDown={(event) => {
            // With the popover up, the arrows, Enter and Tab belong to it and
            // Escape hands them back. Without it, Enter adds the task.
            if (showPopover) {
              if (event.key === "ArrowDown") {
                event.preventDefault();
                setLit((i) => (i + 1) % options);
                return;
              }
              if (event.key === "ArrowUp") {
                event.preventDefault();
                setLit((i) => (i - 1 + options) % options);
                return;
              }
              if (event.key === "Enter" || event.key === "Tab") {
                event.preventDefault();
                acceptLit();
                return;
              }
              if (event.key === "Escape") {
                event.preventDefault();
                setCaret(0);
                return;
              }
            }
            if (event.key === "Enter") run(() => submitDraft());
          }}
          // The selected chip is the default home, and the field says so: an
          // inherited area you were not told about is a surprise later.
          placeholder={filterName ? `Add a task to ${filterName}` : "Add a task"}
          aria-label="Add a task. Type # to choose an area."
          aria-expanded={showPopover}
          aria-autocomplete="list"
          className="ritual-field w-full rounded-full border border-[var(--color-border-subtle)] bg-[var(--color-fill-subtle)] px-5 py-3 text-sm text-[var(--color-ink)] outline-none placeholder:text-[var(--color-ink-muted)] focus:border-[color-mix(in_srgb,var(--color-accent)_60%,transparent)]"
        />
        {/* Todoist's convention, which is the one people already have in their
            hands: type # and the areas appear; keep typing to narrow; Enter or
            Tab to take one. Mouse picks use mousedown so the field keeps
            focus and the caret survives. */}
        {showPopover && tag && draftField.current && (
          <Popover
            anchor={draftField.current}
            align="start"
            role="listbox"
            onClose={closeCompletions}
            className="min-w-40"
          >
            {suggestions.map((context, index) => (
              <li
                key={context.id}
                role="option"
                aria-selected={index === lit}
                onMouseDown={(event) => {
                  event.preventDefault();
                  completeTag(context.name);
                }}
                className={clsx(
                  "cursor-pointer rounded-lg px-2.5 py-1.5 text-[var(--color-ink)]",
                  index === lit && "bg-[var(--color-fill-selected)]",
                )}
              >
                <span className="text-[var(--color-ink-muted)]">#</span>
                {context.name}
              </li>
            ))}
            {canMake && (
              <li
                role="option"
                aria-selected={lit === suggestions.length}
                onMouseDown={(event) => {
                  event.preventDefault();
                  acceptLit(suggestions.length);
                }}
                className={clsx(
                  "cursor-pointer rounded-lg px-2.5 py-1.5 text-[var(--color-ink-muted)]",
                  lit === suggestions.length && "bg-[var(--color-fill-selected)]",
                )}
              >
                New area “{tag.query}”
              </li>
            )}
          </Popover>
        )}
        </div>
        <input
          value={search}
          onChange={(event) => setSearch(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Escape") setSearch("");
          }}
          placeholder="Search"
          aria-label="Search all tasks"
          className="ritual-field min-w-40 flex-[0_1_14rem] rounded-full border border-[var(--color-border-subtle)] bg-[var(--color-fill-subtle)] px-5 py-3 text-sm text-[var(--color-ink)] outline-none placeholder:text-[var(--color-ink-muted)] focus:border-[color-mix(in_srgb,var(--color-accent)_60%,transparent)]"
        />
      </div>

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
            onClick={notice.action.run}
            className="ritual-pressable shrink-0 rounded-full border border-[color-mix(in_srgb,var(--color-ink)_16%,transparent)] px-2.5 py-1 text-xs text-[var(--color-ink)]"
          >
            {notice.action.label}
          </button>
        </div>
      )}

        <div className="min-h-0 flex-1 overflow-auto">
          {query !== "" ? (
            <SearchResults
              results={results}
              query={search.trim()}
              onToggleDone={(t) => run(() => toggleDone(t))}
              onFocus={(t) => run(() => focus(t))}
              onDelete={(t) => run(() => deleteTask(t))}
              activeTaskId={activeTaskId}
              sessionRunning={sessionRunning}
            />
          ) : view === "matrix" ? (
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
    </AreasContext.Provider>
  );
}

/**
 * Every task that matches, each carrying where it lives - the quadrant's own
 * tint on the card and its name in small caps. On the board position is the
 * category; a result has left its position behind, so it brings the answer.
 */
function SearchResults({
  results,
  query,
  onToggleDone,
  onFocus,
  onDelete,
  activeTaskId,
  sessionRunning,
}: {
  results: { task: Task; place: { zone: string; label: string } }[];
  query: string;
  onToggleDone: (task: Task) => void;
  onFocus: (task: Task) => void;
  onDelete: (task: Task) => void;
  activeTaskId: number | null;
  sessionRunning: boolean;
}) {
  if (results.length === 0) {
    return (
      <p className="text-sm text-[var(--color-ink-muted)]">
        Nothing matches “{query}”.
      </p>
    );
  }

  return (
    <div className="flex flex-col gap-2">
      {/* aria-live, so a screen reader hears the count change as the query
          narrows without having to leave the search field to check. */}
      <p aria-live="polite" className="text-xs text-[var(--color-ink-muted)]">
        {results.length === 1 ? "One match" : `${results.length} matches`}
      </p>
      {results.map(({ task, place }) => (
        // The wrapper borrows the zone's identity the same way a zone does:
        // data-zone sets --zone-fill, .search-hit maps it to --zone-bg, and
        // the card inside needs to know nothing about search.
        <div key={task.id} data-zone={place.zone} className="search-hit">
          <TaskCard
            task={task}
            place={place.label}
            sortable={false}
            onToggleDone={onToggleDone}
            onFocus={onFocus}
            onDelete={onDelete}
            active={task.id === activeTaskId}
            sessionRunning={sessionRunning}
          />
        </div>
      ))}
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
      name: "No area",
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

/**
 * A filter that is also a drop target. `dropId` registers it with the drag
 * layer; a card dropped here changes its area. The ring while a card hovers
 * is the zone's own accent ring at chip scale, so "you can drop this here"
 * reads the same everywhere on the page.
 */
function Chip({
  children,
  active,
  onClick,
  dropId,
}: {
  children: React.ReactNode;
  active: boolean;
  onClick: () => void;
  dropId: string;
}) {
  const { setNodeRef, isOver } = useDroppable({ id: dropId });
  return (
    <button
      ref={setNodeRef}
      type="button"
      onClick={onClick}
      data-over={isOver || undefined}
      className={clsx(
        "area-chip rounded-full border px-3 py-1.5 text-xs transition-[color,border-color,box-shadow] duration-150",
        active
          ? "border-[var(--color-accent)] text-[var(--color-ink)]"
          : "border-[var(--color-border-subtle)] text-[var(--color-ink-muted)] hover:text-[var(--color-ink)]",
      )}
    >
      {children}
    </button>
  );
}
