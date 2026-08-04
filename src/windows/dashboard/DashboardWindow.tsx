import { useEffect, useState } from "react";
import clsx from "clsx";
import { pauseStatus, resumeNow } from "@/lib/tauri";
import { useAppearance } from "@/appearance/useAppearance";
import { useSession } from "@/session/useSession";
import { Banner, BannerAction } from "./Banner";
import { BlockOnlyBanner, SessionBanner } from "./SessionBanner";
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
  // Tasks first: the list is what you came to change, whereas the overview is
  // something you read occasionally.
  const [tab, setTab] = useState<Tab>("tasks");
  const { appearance, setAppearance } = useAppearance();
  const { status, end: endSession } = useSession();
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
                ? "bg-[var(--color-fill-selected)] text-[var(--color-ink)]"
                : "text-[var(--color-ink-muted)] hover:text-[var(--color-ink)]",
            )}
          >
            {option}
          </button>
        ))}
      </nav>

      {/* One strip, never two.
          A pause stops everything the app exists to do, so it outranks a
          countdown — the same precedence the tray tooltip already uses. Both
          being true should be impossible (a session cannot start while paused),
          which is exactly why it is handled rather than assumed away. */}
      {pausedUntil !== null ? (
        <Banner
          action={
            <BannerAction
              onClick={() => {
                void resumeNow().then(() => setPausedUntil(null));
              }}
            >
              Resume now
            </BannerAction>
          }
        >
          Threshold is paused until{" "}
          {new Date(pausedUntil * 1000).toLocaleString()}. Nothing will
          interrupt you until then.
        </Banner>
      ) : status?.session ? (
        <SessionBanner session={status.session} onEnd={endSession} />
      ) : status?.blockSeconds ? (
        <BlockOnlyBanner seconds={status.blockSeconds} />
      ) : null}

      <div className="min-h-0 flex-1">
        {tab === "overview" && <OverviewView />}
        {tab === "tasks" && (
          <TasksView
            activeTaskId={status?.session?.taskId ?? null}
            sessionRunning={Boolean(status?.session)}
          />
        )}
        {tab === "settings" && (
          <SettingsView
            appearance={appearance}
            onAppearanceChange={setAppearance}
          />
        )}
      </div>
    </div>
  );
}
