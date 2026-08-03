import { useState } from "react";
import clsx from "clsx";
import { TasksView } from "./tasks/TasksView";
import { OverviewView } from "./OverviewView";

type Tab = "overview" | "tasks";

/**
 * One window, two tabs. Keeping the dashboard and the list in the same WebView
 * is the difference between one webview process and two, which matters when the
 * whole app is supposed to sit near-invisibly in the tray all day.
 */
export function DashboardWindow() {
  const [tab, setTab] = useState<Tab>("overview");

  return (
    <div className="flex h-full flex-col bg-[var(--color-surface)]">
      <nav className="flex items-center gap-1 border-b border-[var(--color-border-subtle)] px-8 py-3">
        <h1 className="mr-4 text-sm font-medium tracking-tight">Threshold</h1>
        {(["overview", "tasks"] as const).map((option) => (
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

      <div className="min-h-0 flex-1">
        {tab === "overview" ? <OverviewView /> : <TasksView />}
      </div>
    </div>
  );
}
