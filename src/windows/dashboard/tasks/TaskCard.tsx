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
  active = false,
  sessionRunning = false,
}: {
  task: Task;
  onToggleDone: (task: Task) => void;
  onFocus?: (task: Task) => void;
  /** A session is running on *this* task. */
  active?: boolean;
  /** A session is running on some task, this one or another. */
  sessionRunning?: boolean;
}) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } =
    useSortable({ id: task.id });

  const done = task.status === "done";

  return (
    // The card derives its surface from the zone it sits in (--zone-bg), so it
    // needs no knowledge of quadrants and still works in the list view via the
    // :root fallback. Dragging and done are attributes rather than opacity:
    // fading a card over a coloured zone smears the hue through it and reads
    // differently in every quadrant.
    <div
      ref={setNodeRef}
      data-dragging={isDragging || undefined}
      data-done={done || undefined}
      data-active={(active && !done) || undefined}
      style={{
        transform: CSS.Transform.toString(transform),
        transition,
      }}
      className={clsx(
        "matrix-card group flex items-center gap-3 rounded-xl border px-3 py-2.5 text-sm",
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
            : // Muted ink at 50% measures 2.11:1 on a Do First card — under the
              // 3:1 a control boundary needs. Derived from the zone's own muted
              // ink instead, which clears it on every quadrant.
              "border-[color-mix(in_srgb,var(--zone-ink-muted)_70%,transparent)] hover:border-[var(--color-accent)]",
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
          buttons should move it. `min-w-0` lets it actually shrink — without it
          `truncate` gives the span its full text width, which pushes the Focus
          button out past the edge of the card on long titles. */}
      <span
        {...attributes}
        {...listeners}
        className={clsx(
          "min-w-0 flex-1 cursor-grab truncate active:cursor-grabbing",
          done && "text-[var(--zone-ink-muted)] line-through",
        )}
      >
        {task.title}
      </span>

      {/* Always visible rather than revealed on hover: a control you cannot see
          is a control most people never find. */}
      {onFocus &&
        !done &&
        (active ? (
          // Not a button. There is one way to end a session and it lives in the
          // banner, next to the number that explains why you would.
          <span className="shrink-0 rounded-full border border-[color-mix(in_srgb,var(--color-accent)_55%,transparent)] px-2.5 py-1 text-xs text-[var(--color-ink)]">
            Running
          </span>
        ) : (
          <button
            type="button"
            // Still pressable while another session runs. A greyed-out control
            // tells you nothing; one that answers tells you why, and the answer
            // lands in the error line TasksView already renders.
            aria-disabled={sessionRunning || undefined}
            onClick={() => onFocus(task)}
            // Border derived from ink rather than the white-alpha token, which is
            // invisible in light mode and near-invisible on a tinted card.
            className={clsx(
              "ritual-pressable shrink-0 rounded-full border border-[color-mix(in_srgb,var(--color-ink)_16%,transparent)] px-2.5 py-1 text-xs",
              sessionRunning
                ? "text-[color-mix(in_srgb,var(--zone-ink-muted)_65%,transparent)]"
                : "text-[var(--zone-ink-muted)] hover:text-[var(--color-ink)]",
            )}
          >
            Focus
          </button>
        ))}
    </div>
  );
}
