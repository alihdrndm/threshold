import { useEffect, useState } from "react";

/**
 * Minutes left, rounded up.
 *
 * Ceil so it never reads 0 while time remains, and never claims a precision it
 * does not have. The banner says "about N minutes" for the same reason.
 */
export function minutesLeft(endsTs: number, now = Date.now()): number {
  return Math.max(0, Math.ceil((endsTs * 1000 - now) / 60_000));
}

/**
 * A countdown that updates once a minute, and only when the number changes.
 *
 * Always recomputed from `endsTs` against the wall clock — never by
 * decrementing a stored count, which drifts to nonsense the first time the
 * machine sleeps. That is the same defect the debounce had, and it is not worth
 * reintroducing in the UI.
 *
 * Minute resolution is a design decision, not a shortcut. A second-by-second
 * countdown in the corner of the screen is an attention magnet, which is the
 * exact thing this app exists to remove. There is no progress bar either: a bar
 * that visibly fills is a countdown with better graphics.
 */
export function useMinutesLeft(endsTs: number): number {
  const [minutes, setMinutes] = useState(() => minutesLeft(endsTs));

  useEffect(() => {
    let timer: number;

    const tick = () => {
      setMinutes(minutesLeft(endsTs));
      // Aim at the next moment the displayed number actually changes, rather
      // than a fixed cadence that drifts a little further from the truth on
      // every pass.
      const remaining = endsTs * 1000 - Date.now();
      const delay = ((remaining % 60_000) + 60_000) % 60_000 || 60_000;
      timer = window.setTimeout(tick, delay + 250);
    };
    tick();

    // WebView2 throttles timers hard in a hidden window and stops them when
    // minimised — and this dashboard is *hidden*, not destroyed, most of the
    // day. That can only make the number stale, never wrong, because it is
    // always derived from endsTs. So recompute the moment it is looked at.
    const resync = () => {
      if (document.visibilityState !== "visible") return;
      window.clearTimeout(timer);
      tick();
    };
    document.addEventListener("visibilitychange", resync);
    window.addEventListener("focus", resync);

    return () => {
      window.clearTimeout(timer);
      document.removeEventListener("visibilitychange", resync);
      window.removeEventListener("focus", resync);
    };
  }, [endsTs]);

  return minutes;
}
