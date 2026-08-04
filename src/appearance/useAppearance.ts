import { useCallback, useEffect, useState } from "react";
import { getSettings, isTauri, setSetting, setWindowTheme } from "@/lib/tauri";
import {
  APPEARANCE_KEY,
  apply,
  isAppearance,
  readMirror,
  resolve,
  systemPrefersDark,
  writeMirror,
  type Appearance,
  type Resolved,
} from ".";

/**
 * The appearance preference, resolved and applied.
 *
 * Called by `DashboardWindow` and nothing else, which is what makes
 * "dashboard only" a structural guarantee rather than a window-label check
 * duplicated from the registry.
 */
export function useAppearance(): {
  appearance: Appearance;
  resolved: Resolved;
  setAppearance: (next: Appearance) => void;
} {
  // Seeded from the same mirror the inline bootstrap read, so mounting never
  // re-applies and never flickers.
  const [appearance, setState] = useState<Appearance>(readMirror);
  const [systemDark, setSystemDark] = useState(systemPrefersDark);

  // The database is authoritative; the mirror is the cache that lets the first
  // paint happen before IPC exists. Reconcile once, quietly.
  useEffect(() => {
    getSettings()
      .then((pairs) => {
        const stored = pairs.find(([key]) => key === APPEARANCE_KEY)?.[1];
        if (isAppearance(stored) && stored !== readMirror()) {
          setState(stored);
          writeMirror(stored);
        }
      })
      .catch(() => {
        // No backend (plain browser dev). The mirror is the whole truth there.
      });
  }, []);

  // Only subscribe while the preference is actually "system". An explicit
  // choice means the OS is irrelevant, and no listener is both correct and free.
  useEffect(() => {
    if (appearance !== "system") return;
    const query = window.matchMedia("(prefers-color-scheme: dark)");
    // Seed first, to catch a change that happened while unsubscribed.
    setSystemDark(query.matches);
    const onChange = (event: MediaQueryListEvent) => setSystemDark(event.matches);
    query.addEventListener("change", onChange);
    return () => query.removeEventListener("change", onChange);
  }, [appearance]);

  const resolved = resolve(appearance, systemDark);

  useEffect(() => {
    apply(resolved);
  }, [resolved]);

  // The native titlebar is the one surface CSS cannot reach. `null` means
  // "follow Windows", which also leaves the webview's prefers-color-scheme free
  // so that "system" keeps tracking the OS live.
  useEffect(() => {
    if (!isTauri()) return;
    void setWindowTheme(appearance === "system" ? null : appearance).catch(
      () => {},
    );
  }, [appearance]);

  const setAppearance = useCallback((next: Appearance) => {
    setState(next);
    writeMirror(next);
    void setSetting(APPEARANCE_KEY, next).catch(() => {});
  }, []);

  return { appearance, resolved, setAppearance };
}
