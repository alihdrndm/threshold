import { useLayoutEffect, useRef, useState } from "react";

/**
 * How wide an element is, in CSS pixels, from the element itself rather than
 * the window - the dashboard has padding and may one day have a sidebar, and
 * a layout should answer to the space it actually has.
 *
 * Measured before the first paint, then watched: a board that rendered narrow
 * for one frame and snapped wide would read as a glitch on every visit.
 * Shared by the matrix board and the week panel.
 */
export function useWidth<T extends HTMLElement>(): [
  React.RefObject<T | null>,
  number,
] {
  const ref = useRef<T>(null);
  const [width, setWidth] = useState(0);
  useLayoutEffect(() => {
    const node = ref.current;
    if (!node) return;
    setWidth(node.getBoundingClientRect().width);
    const observer = new ResizeObserver(([entry]) => {
      setWidth(entry.contentRect.width);
    });
    observer.observe(node);
    return () => observer.disconnect();
  }, []);
  return [ref, width];
}
