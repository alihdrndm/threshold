import { useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import clsx from "clsx";
import type { Task } from "@/lib/tauri";
import { AreaControl } from "./AreaMenu";
import { SlotControl } from "./SlotControl";

/**
 * A draggable task.
 *
 * The transform comes from dnd-kit as a string rather than through Motion's
 * shorthand props, so the drag stays on the compositor while the rest of the
 * app is busy.
 */
export function TaskCard({
  task,
  ordinal,
  place,
  sortable = true,
  onToggleDone,
  onFocus,
  onDelete,
  active = false,
  sessionRunning = false,
}: {
  task: Task;
  /** 1-based place in the zone. Present on the matrix, absent in the list. */
  ordinal?: number;
  /** Where the task lives, named. Search results carry it; the board is it. */
  place?: string;
  /** False in search results: a flat filtered list has no order to rearrange. */
  sortable?: boolean;
  onToggleDone: (task: Task) => void;
  onFocus?: (task: Task) => void;
  onDelete?: (task: Task) => void;
  /** A session is running on *this* task. */
  active?: boolean;
  /** A session is running on some task, this one or another. */
  sessionRunning?: boolean;
}) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } =
    useSortable({ id: task.id, disabled: !sortable });

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
        // items-start, not items-center: a title is allowed to wrap now, and a
        // centred checkbox beside a three-line title floats in the middle of
        // nowhere. The small top margins below re-centre every control against
        // the title's first line instead.
        //
        // flex-wrap is the responsive rule, and it is one rule: the title
        // claims at least 9rem, and when the controls no longer fit beside
        // that they drop to a second line, right-aligned. Before this a narrow
        // rail squeezed the title to a few dozen pixels and broke words in
        // half - "Refin / e / Thres / hold" - which is not wrapping, it is
        // damage.
        "matrix-card group flex flex-wrap items-start gap-x-3 gap-y-2 rounded-xl border px-3 py-2.5 text-sm",
      )}
    >
      {/* The place, not a bullet: it changes when the card is dragged, which
          is what makes the order feel real enough to rearrange. aria-hidden
          because dnd-kit already announces position while sorting, and a list
          numbered twice reads twice as slowly. Width reserves two digits so
          ten tasks do not push every title a pixel sideways. */}
      {ordinal !== undefined && (
        <span
          aria-hidden
          className="mt-1 w-4 shrink-0 text-right text-xs text-[var(--zone-ink-muted)] tabular-nums select-none"
        >
          {ordinal}
        </span>
      )}

      <button
        type="button"
        onClick={() => onToggleDone(task)}
        aria-label={done ? "Mark as not done" : "Mark as done"}
        className={clsx(
          "mt-1 grid size-[18px] shrink-0 place-items-center rounded-full border transition-[color,background-color,border-color,transform] duration-150 active:scale-[0.97]",
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
          buttons should move it. The title wraps rather than truncates — a
          clipped title hides exactly the words that distinguish two similar
          tasks, and this list is short by design so the space is affordable.
          `min-w-0` still matters: without it the span takes its full text
          width and pushes the buttons past the edge of the card. */}
      <span
        {...(sortable ? attributes : {})}
        {...(sortable ? listeners : {})}
        className={clsx(
          "mt-[3px] min-w-36 flex-1 leading-snug break-words",
          sortable && "cursor-grab active:cursor-grabbing",
          done && "text-[var(--zone-ink-muted)] line-through",
        )}
      >
        {task.title}
      </span>

      {/* The controls travel together: beside the title when there is room,
          under it and pushed right when there is not. */}
      <div className="ml-auto flex shrink-0 items-center gap-3">
      {/* Quieter and smaller than the zone headers that share its vocabulary:
          on the board the category is a place you look at, here it is a fact
          attached to someone else's answer. */}
      {place && (
        <span className="text-[10px] tracking-[0.14em] uppercase text-[var(--zone-ink-muted)] select-none">
          {place}
        </span>
      )}

      {/* The slot, when this task lives in Schedule (not urgent, important).
          Read before the area, the way a calendar time is the first thing you
          look for on a scheduled thing. */}
      {!done && task.urgent === false && task.important === true && (
        <SlotControl task={task} />
      )}

      {/* The one home this task has, or the way to give it one. Before Focus:
          where a thing belongs is read before what to do with it. */}
      {!done && <AreaControl task={task} />}

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

      {/* Quiet on purpose, hidden never: the same rule as Focus. It carries no
          border because it is not an invitation, only an exit — and deleting is
          undoable from the notice line, which is why there is no "are you
          sure". A dialog guards against a click; an undo forgives one. */}
      {onDelete && !active && (
        <button
          type="button"
          onClick={() => onDelete(task)}
          aria-label={`Delete "${task.title}"`}
          className="grid size-6 shrink-0 place-items-center rounded-full text-[color-mix(in_srgb,var(--zone-ink-muted)_75%,transparent)] transition-[color,background-color,transform] duration-150 hover:bg-[color-mix(in_srgb,var(--color-ink)_8%,transparent)] hover:text-[var(--color-ink)] active:scale-[0.97]"
        >
          <svg viewBox="0 0 12 12" className="size-3" aria-hidden>
            <path
              d="M3 3 9 9 M9 3 3 9"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.4"
              strokeLinecap="round"
            />
          </svg>
        </button>
      )}
      </div>
    </div>
  );
}
