import { useEffect, useState } from "react";
import clsx from "clsx";
import { pauseStatus, resumeNow } from "@/lib/tauri";
import { TasksView } from "./tasks/TasksView";
import { OverviewView } from "./OverviewView";
import { SettingsView } from "./SettingsView";

type Tab = "overview" | "tasks" | "settings";

/**
 * One window, two tabs. Keeping the dashboard and the list in the same WebView
 * is the difference between one webview process and two, which matters when the
 * whole app is supposed to sit near-invisibly in the tray all day.
 */
export function DashboardWindow() {
  const [tab, setTab] = useState<Tab>("overview");
  const [pausedUntil, setPausedUntil] = useState<number | null>(null);

  // Re-checked when the window opens and whenever a tab changes, so resuming
  // from Settings clears the banner without a reload.
  useEffect(() => {
    pauseStatus()
      .then(setPausedUntil)
      .catch(() => setPausedUntil(null));
  }, [tab]);

  return (
    <div className="flex h-full flex-col bg-[var(--color-surface)]">
      <nav className="flex items-center gap-1 border-b border-[var(--color-border-subtle)] px-8 py-3">
        <h1 className="mr-4 text-sm font-medium tracking-tight">Threshold</h1>
        {(["overview", "tasks", "settings"] as const).map((option) => (
          <button
            key={option}
            type="button"
            onClick={() => setTab(option)}
            className={clsx(
              "rounded-full px-4 py-1.5 text-sm capitalize transition-colors duration-150",
              tab === option
                ? "bg-white/10 text-[var(--color-ink)]"
                : "text-[var(--color-ink-muted)] hover:text-[var(--color-ink)]",
            )}
          >
            {option}
          </button>
        ))}
      </nav>

      {/* A pause stops everything the app exists to do. It belongs across the
          top of every tab, not folded into a settings panel nobody opens. */}
      {pausedUntil !== null && (
        <div className="flex items-center gap-4 border-b border-[var(--color-accent)]/40 bg-[var(--color-accent)]/[0.08] px-8 py-3">
          <p className="flex-1 text-sm">
            Threshold is paused until{" "}
            {new Date(pausedUntil * 1000).toLocaleString()}. Nothing will
            interrupt you until then.
          </p>
          <button
            type="button"
            onClick={() => {
              void resumeNow().then(() => setPausedUntil(null));
            }}
            className="ritual-pressable rounded-full border border-[var(--color-border-subtle)] px-4 py-1.5 text-sm"
          >
            Resume now
          </button>
        </div>
      )}

      <div className="min-h-0 flex-1">
        {tab === "overview" && <OverviewView />}
        {tab === "tasks" && <TasksView />}
        {tab === "settings" && <SettingsView />}
      </div>
    </div>
  );
}
