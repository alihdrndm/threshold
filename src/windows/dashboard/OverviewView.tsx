import { useEffect, useMemo, useState } from "react";
import { dataLocation, recentIntentions, type IntentionRow } from "@/lib/tauri";

/**
 * The dashboard (spec F4): one screen, four numbers, one list.
 *
 * Deliberately not a charting surface. Tracking on its own does not change
 * behaviour, so this is a mirror that supports the ritual rather than a product
 * competing with it. If it ever grows a graph, something has gone wrong.
 */
export function OverviewView() {
  const [rows, setRows] = useState<IntentionRow[]>([]);
  const [where, setWhere] = useState("");
  const [query, setQuery] = useState("");

  useEffect(() => {
    recentIntentions(200)
      .then(setRows)
      .catch(() => setRows([]));
    dataLocation()
      .then(setWhere)
      .catch(() => setWhere(""));
  }, []);

  const stats = useMemo(() => summarise(rows), [rows]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return rows;
    return rows.filter(
      (row) =>
        row.text?.toLowerCase().includes(q) ||
        row.ifThen?.toLowerCase().includes(q),
    );
  }, [rows, query]);

  return (
    <div className="flex h-full flex-col gap-6 overflow-auto p-8">
      <div className="grid grid-cols-2 gap-4 sm:grid-cols-4">
        <Stat
          label="Streak"
          value={stats.streak === 0 ? "—" : `${stats.streak}d`}
          dim={stats.missedYesterday}
          note={
            stats.missedYesterday
              ? "yesterday was missed — one miss does not break a habit"
              : undefined
          }
        />
        <Stat label="Time reclaimed" value={formatMinutes(stats.reclaimedMin)} />
        <Stat
          label="Sessions"
          value={`${stats.completed}/${stats.completed + stats.browsing}`}
          note="completed vs just browsing"
        />
        <Stat label="Drift" value={String(stats.drifted)} />
      </div>

      <section className="flex min-h-0 flex-1 flex-col gap-3">
        <div className="flex items-baseline justify-between gap-4">
          <h2 className="text-sm font-medium">Intentions</h2>
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search"
            className="ritual-field rounded-full border border-[var(--color-border-subtle)] bg-[var(--color-fill-subtle)] px-4 py-1.5 text-xs outline-none placeholder:text-[var(--color-ink-muted)] focus:border-[color-mix(in_srgb,var(--color-accent)_60%,transparent)]"
          />
        </div>

        {filtered.length === 0 ? (
          <p className="text-sm text-[var(--color-ink-muted)]">
            Nothing recorded yet.
          </p>
        ) : (
          <ul className="flex flex-col gap-1.5">
            {filtered.slice(0, 60).map((row) => (
              <li
                key={row.id}
                className="flex items-baseline gap-3 rounded-lg border border-[var(--color-border-subtle)] px-3 py-2 text-sm"
              >
                <span className="w-28 shrink-0 text-xs text-[var(--color-ink-muted)]">
                  {new Date(row.ts).toLocaleDateString(undefined, {
                    month: "short",
                    day: "numeric",
                    hour: "numeric",
                    minute: "2-digit",
                  })}
                </span>
                <span className="flex-1 truncate">
                  {row.text ?? <span className="text-[var(--color-ink-muted)]">—</span>}
                </span>
                <span className="text-xs text-[var(--color-ink-muted)]">
                  {row.outcome}
                  {row.durationMin ? ` · ${row.durationMin}m` : ""}
                </span>
              </li>
            ))}
          </ul>
        )}
      </section>

      {where && (
        <p className="text-xs text-[var(--color-ink-muted)]">
          Everything above lives on this machine only, in {where}
        </p>
      )}
    </div>
  );
}

function Stat({
  label,
  value,
  note,
  dim = false,
}: {
  label: string;
  value: string;
  note?: string;
  dim?: boolean;
}) {
  return (
    <div className="rounded-2xl border border-[var(--color-border-subtle)] bg-[var(--color-fill-subtle)] p-4">
      <p className="text-xs tracking-[0.16em] text-[var(--color-ink-muted)] uppercase">
        {label}
      </p>
      <p
        className={`mt-2 text-2xl font-light ${dim ? "text-[var(--color-ink-muted)]" : "text-[var(--color-ink)]"}`}
      >
        {value}
      </p>
      {note && (
        <p className="mt-1 text-xs text-[var(--color-ink-muted)]">{note}</p>
      )}
    </div>
  );
}

function formatMinutes(total: number): string {
  if (total <= 0) return "—";
  const hours = Math.floor(total / 60);
  const minutes = total % 60;
  return hours > 0 ? `${hours}h ${minutes}m` : `${minutes}m`;
}

interface Summary {
  streak: number;
  missedYesterday: boolean;
  reclaimedMin: number;
  completed: number;
  browsing: number;
  drifted: number;
}

/**
 * Streaks forgive a single miss.
 *
 * Habit automaticity takes a median of 66 days and survives the odd gap, so a
 * missed day dims the number rather than zeroing it. Two consecutive misses do
 * reset it — otherwise the streak stops meaning anything.
 */
export function summarise(rows: IntentionRow[]): Summary {
  const days = new Set(
    rows
      .filter((row) => row.outcome === "completed")
      .map((row) => new Date(row.ts).toDateString()),
  );

  const dayMs = 86_400_000;
  const today = new Date();
  let streak = 0;
  let misses = 0;
  let missedYesterday = false;

  for (let i = 0; i < 400; i++) {
    const day = new Date(today.getTime() - i * dayMs).toDateString();
    if (days.has(day)) {
      streak++;
      misses = 0;
    } else if (i === 0) {
      // Today is still in progress; not yet a miss.
      continue;
    } else {
      misses++;
      if (i === 1) missedYesterday = true;
      if (misses >= 2) break;
    }
  }

  return {
    streak,
    missedYesterday,
    reclaimedMin: rows
      .filter((row) => row.outcome === "completed")
      .reduce((total, row) => total + (row.durationMin ?? 0), 0),
    completed: rows.filter((row) => row.outcome === "completed").length,
    browsing: rows.filter((row) => row.outcome === "browsing").length,
    drifted: rows.filter((row) => row.outcome === "drifted").length,
  };
}
