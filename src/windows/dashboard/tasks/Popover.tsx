import { useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import clsx from "clsx";

/**
 * A small floating list, rendered at the document root and placed against
 * the control that opened it.
 *
 * Portalled rather than nested on purpose. Nested, a menu on a card lived
 * inside that card's stacking context: the next card - which dnd-kit gives a
 * transform, and so a context of its own - painted straight over it, and the
 * board's scrolling container clipped whatever fell past its edge. Fixed
 * positioning at the root escapes both; the price is measuring the anchor,
 * which is paid once per open.
 *
 * Flips upward when there is no room below, so a menu on the last card of a
 * tall zone does not open into the void beneath the window.
 */
export function Popover({
  anchor,
  align = "end",
  role,
  onClose,
  className,
  children,
}: {
  anchor: HTMLElement;
  /** Which edge of the anchor the list lines up with. */
  align?: "start" | "end";
  role: "menu" | "listbox";
  onClose: () => void;
  className?: string;
  children: React.ReactNode;
}) {
  const list = useRef<HTMLUListElement>(null);
  const [style, setStyle] = useState<React.CSSProperties>({ visibility: "hidden" });

  useLayoutEffect(() => {
    const place = () => {
      const a = anchor.getBoundingClientRect();
      const height = list.current?.offsetHeight ?? 0;
      const gap = 4;
      const below = a.bottom + gap + height <= window.innerHeight;
      const top = below ? a.bottom + gap : Math.max(gap, a.top - gap - height);
      setStyle({
        position: "fixed",
        top,
        ...(align === "end"
          ? { right: Math.max(gap, window.innerWidth - a.right) }
          : { left: Math.max(gap, a.left) }),
        transformOrigin: `${below ? "top" : "bottom"} ${align === "end" ? "right" : "left"}`,
      });
    };
    place();

    // Anything that moves the anchor - a scroll of any container, a resize -
    // closes the menu rather than chasing it. A menu that drifts from its
    // control is worse than one that quietly steps aside.
    const away = (event: PointerEvent) => {
      const target = event.target as Node;
      if (!list.current?.contains(target) && !anchor.contains(target)) onClose();
    };
    const key = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    document.addEventListener("pointerdown", away);
    document.addEventListener("keydown", key);
    window.addEventListener("scroll", onClose, true);
    window.addEventListener("resize", onClose);
    return () => {
      document.removeEventListener("pointerdown", away);
      document.removeEventListener("keydown", key);
      window.removeEventListener("scroll", onClose, true);
      window.removeEventListener("resize", onClose);
    };
  }, [anchor, align, onClose]);

  return createPortal(
    <ul
      ref={list}
      role={role}
      style={style}
      className={clsx(
        "area-menu z-50 flex min-w-36 flex-col rounded-xl border border-[var(--color-border-subtle)] bg-[var(--color-surface-raised)] p-1 text-sm shadow-[0_12px_32px_-12px_rgb(0_0_0/0.5)]",
        className,
      )}
    >
      {children}
    </ul>,
    document.body,
  );
}
