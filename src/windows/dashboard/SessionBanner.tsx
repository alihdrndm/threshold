import { useMinutesLeft } from "@/session/useMinutesLeft";
import type { ActiveSession } from "@/lib/tauri";
import { Banner, BannerAction } from "./Banner";

/**
 * What you are on, and roughly how long is left.
 *
 * Its own component for one reason that is not stylistic: it re-renders every
 * minute, and the dashboard root must not. A root re-render while dnd-kit has
 * a pointer captured hands it a new tree mid-drag and the drag is dropped.
 *
 * The task is named and the blocklist is not. Foregrounding what you are
 * avoiding makes it more available, not less — so this says what you are doing.
 */
export function SessionBanner({
  session,
  onEnd,
}: {
  session: ActiveSession;
  onEnd: () => void;
}) {
  const minutes = useMinutesLeft(session.endsTs);
  const subject = session.subject?.trim();

  return (
    <Banner
      action={
        <BannerAction
          onClick={onEnd}
          label={subject ? `End the session on ${subject}` : "End the session"}
        >
          End session
        </BannerAction>
      }
    >
      {subject ? `Focusing on ${subject}.` : "A session is running."}{" "}
      {/* "About" says this is a horizon, not a stopwatch — and stops a rounded
          minute reading as a clock that is slightly wrong. */}
      {minutes > 0 ? `About ${minutes} minutes left.` : "Under a minute left."}
      {session.categories.length > 0 && (
        <span className="text-[var(--color-ink-muted)]"> Sites are quiet until then.</span>
      )}
    </Banner>
  );
}

/**
 * A block with no session behind it.
 *
 * Reachable two ways: upgrading mid-commitment, and ending a session early
 * while the helper still owes time on its own clock. Both need saying out loud,
 * because the alternative is sites that do not load and nothing on screen that
 * explains why.
 */
export function BlockOnlyBanner({ seconds }: { seconds: number }) {
  const minutes = Math.max(1, Math.ceil(seconds / 60));
  return (
    <Banner>
      Your session is over, but the sites stay quiet for about {minutes} more{" "}
      {minutes === 1 ? "minute" : "minutes"} — that was the commitment. Settings
      has a way out if you need one.
    </Banner>
  );
}
