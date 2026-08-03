import { useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import clsx from "clsx";
import type { Task } from "@/lib/tauri";

/**
 * A draggable task.
 *
 * The transform comes from dnd-kit as a string rather than through Motion's
 * shorthand props, so the drag stays on the compositor while the rest of the
 * app is busy.
 */
export function TaskCard({
  task,
  onToggleDone,
  onFocus,
}: {
  task: Task;
  onToggleDone: (task: Task) => void;
  onFocus?: (task: Task) => void;
}) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } =
    useSortable({ id: task.id });

  const done = task.status === "done";

  return (
    <div
      ref={setNodeRef}
      style={{
        transform: CSS.Transform.toString(transform),
        transition,
      }}
      className={clsx(
        "group flex items-center gap-3 rounded-xl border px-3 py-2.5 text-sm",
        "border-[var(--color-border-subtle)] bg-white/[0.03]",
        isDragging && "opacity-60",
        done && "opacity-45",
      )}
    >
      <button
        type="button"
        onClick={() => onToggleDone(task)}
        aria-label={done ? "Mark as not done" : "Mark as done"}
        className={clsx(
          "grid size-[18px] shrink-0 place-items-center rounded-full border transition-colors duration-150",
          done
            ? "border-[var(--color-accent)] bg-[var(--color-accent)]/20"
            : "border-[var(--color-ink-muted)]/50 hover:border-[var(--color-accent)]",
        )}
      >
        {done && (
          <svg viewBox="0 0 12 12" className="size-3" aria-hidden>
            <path
              d="M2.5 6.2 4.7 8.4 9.5 3.6"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.6"
              strokeLinecap="round"
              strokeLinejoin="round"
              className="text-[var(--color-accent)]"
            />
          </svg>
        )}
      </button>

      {/* The whole body is the drag handle: grabbing a task anywhere but the
          checkbox should move it. */}
      <span
        {...attributes}
        {...listeners}
        className={clsx(
          "flex-1 cursor-grab truncate active:cursor-grabbing",
          done && "line-through",
        )}
      >
        {task.title}
      </span>

      {onFocus && !done && (
        <button
          type="button"
          onClick={() => onFocus(task)}
          className="rounded-full border border-[var(--color-border-subtle)] px-2.5 py-1 text-xs text-[var(--color-ink-muted)] opacity-0 transition-opacity duration-150 group-hover:opacity-100 hover:text-[var(--color-ink)] focus-visible:opacity-100"
        >
          Focus
        </button>
      )}
    </div>
  );
}
