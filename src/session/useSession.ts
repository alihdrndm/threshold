import { useCallback, useEffect, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  endSessionEarly,
  isTauri,
  sessionStatus,
  type SessionStatus,
} from "@/lib/tauri";

/**
 * The running session, if there is one.
 *
 * Called by `DashboardWindow` and nothing else; the banner, the matrix and the
 * Focus buttons are all handed it from there. That is what makes "one
 * subscription" a structural fact rather than a convention three components
 * have to remember.
 *
 * Deliberately not polled. The truth arrives three ways and none of them is an
 * interval: once on mount (a session survives an app restart — it lives on
 * disk), on the session-started/ended events, and whenever the window becomes
 * visible again, which covers anything emitted while this webview did not
 * exist. A timer running forever in a window that is hidden most of the day is
 * exactly the idle cost this app is built to avoid.
 */
export function useSession(): {
  status: SessionStatus | null;
  end: () => void;
} {
  const [status, setStatus] = useState<SessionStatus | null>(null);

  const refresh = useCallback(() => {
    sessionStatus()
      .then(setStatus)
      .catch(() => setStatus(null));
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  useEffect(() => {
    if (!isTauri()) return;

    // `listen` resolves after the subscription exists, and StrictMode mounts
    // effects twice in development. Without this flag the first mount's
    // unlisten lands after the second has subscribed, leaving one orphaned
    // listener and no working one.
    let cancelled = false;
    const unsubs: UnlistenFn[] = [];
    const on = (event: string, handler: () => void) =>
      listen(event, handler).then((un) => {
        if (cancelled) un();
        else unsubs.push(un);
      });

    void on("session-started", refresh);
    // Re-read rather than clearing: a block can outlive the session that armed
    // it, and the banner still has something true to say about that.
    void on("session-ended", refresh);

    return () => {
      cancelled = true;
      unsubs.forEach((un) => un());
    };
  }, [refresh]);

  useEffect(() => {
    const onVisible = () => {
      if (document.visibilityState === "visible") refresh();
    };
    document.addEventListener("visibilitychange", onVisible);
    return () => document.removeEventListener("visibilitychange", onVisible);
  }, [refresh]);

  const end = useCallback(() => {
    const id = status?.session?.id;
    if (id === undefined) return;
    // Optimistic: the banner goes the moment you press End. The check-in that
    // follows is the confirmation, and it comes from Rust.
    setStatus((current) => (current ? { ...current, session: null } : current));
    void endSessionEarly(id)
      .then(refresh)
      .catch(refresh);
  }, [status, refresh]);

  return { status, end };
}
