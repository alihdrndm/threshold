import { createContext, useContext, useEffect, useRef, useState } from "react";
import clsx from "clsx";
import type { Task, TaskContext } from "@/lib/tauri";

/**
 * Areas, reachable from any card without threading props through five
 * components. The value is the list and one verb - give this task that area -
 * which is all a card needs to know.
 */
export const AreasContext = createContext<{
  contexts: TaskContext[];
  setArea: (task: Task, contextId: number | null) => void;
} | null>(null);

/**
 * The area control on a card: the area's name when it has one, a small `#`
 * when it does not - always visible, per the rule that a control you cannot
 * see is one most people never find. Pressing either opens the same menu.
 *
 * The `#` is not decoration: it is the quick-add syntax, so the glyph on the
 * card teaches the shortcut in the input.
 */
export function AreaControl({ task }: { task: Task }) {
  const areas = useContext(AreasContext);
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);

  // Close on a click anywhere else, or Escape - a menu that stays open under
  // the next thing you click is a menu you learn to distrust.
  useEffect(() => {
    if (!open) return;
    const away = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    };
    const key = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("pointerdown", away);
    document.addEventListener("keydown", key);
    return () => {
      document.removeEventListener("pointerdown", away);
      document.removeEventListener("keydown", key);
    };
  }, [open]);

  if (!areas) return null;
  const current = areas.contexts.find((c) => c.id === task.contextId) ?? null;

  return (
    <div ref={root} className="relative shrink-0">
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-label={current ? `Area: ${current.name}. Change area` : "Set an area"}
        className={clsx(
          "grid h-6 place-items-center rounded-full transition duration-150 active:scale-95",
          current
            ? "mt-px px-2 text-[10px] tracking-[0.14em] uppercase text-[var(--zone-ink-muted)] hover:text-[var(--color-ink)]"
            : "mt-px w-6 text-xs text-[color-mix(in_srgb,var(--zone-ink-muted)_75%,transparent)] hover:bg-[color-mix(in_srgb,var(--color-ink)_8%,transparent)] hover:text-[var(--color-ink)]",
        )}
      >
        {current ? current.name : "#"}
      </button>

      {open && (
        // The same surface as a zone's raised card, above whatever the card
        // sits on. Enters like every other small thing here: fast, ease-out,
        // from very nearly its final size.
        <ul
          role="menu"
          className="area-menu absolute top-full right-0 z-20 mt-1 flex min-w-36 flex-col rounded-xl border border-[var(--color-border-subtle)] bg-[var(--color-surface-raised)] p-1 text-sm shadow-[0_12px_32px_-12px_rgb(0_0_0/0.5)]"
        >
          {areas.contexts.map((context) => (
            <AreaItem
              key={context.id}
              selected={context.id === task.contextId}
              onPress={() => {
                setOpen(false);
                areas.setArea(task, context.id);
              }}
            >
              {context.name}
            </AreaItem>
          ))}
          <AreaItem
            selected={task.contextId === null}
            muted
            onPress={() => {
              setOpen(false);
              areas.setArea(task, null);
            }}
          >
            No area
          </AreaItem>
        </ul>
      )}
    </div>
  );
}

function AreaItem({
  children,
  selected,
  muted = false,
  onPress,
}: {
  children: React.ReactNode;
  selected: boolean;
  muted?: boolean;
  onPress: () => void;
}) {
  return (
    <li role="none">
      <button
        type="button"
        role="menuitemradio"
        aria-checked={selected}
        onClick={onPress}
        className={clsx(
          "flex w-full items-center justify-between gap-3 rounded-lg px-2.5 py-1.5 text-left transition-colors duration-100 hover:bg-[var(--color-fill-selected)]",
          muted ? "text-[var(--color-ink-muted)]" : "text-[var(--color-ink)]",
        )}
      >
        <span>{children}</span>
        {selected && (
          <span aria-hidden className="text-[var(--color-accent)]">
            •
          </span>
        )}
      </button>
    </li>
  );
}
