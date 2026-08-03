import { useEffect, useState } from "react";
import clsx from "clsx";
import {
  getSettings,
  pauseFor,
  pauseStatus,
  resumeNow,
  setSetting,
} from "@/lib/tauri";
import { CATEGORIES } from "../popup/ritual/copy";

/**
 * Settings (spec F5).
 *
 * Everything here is editable on purpose. Self-set limits are the ones people
 * keep - imposed ones get around 25% compliance and breed workarounds - so the
 * categories, the thresholds and the pause are all yours to change.
 */
export function SettingsView() {
  const [values, setValues] = useState<Record<string, string>>({});
  const [pausedUntil, setPausedUntil] = useState<number | null>(null);

  async function refresh() {
    const pairs = await getSettings();
    setValues(Object.fromEntries(pairs));
    setPausedUntil(await pauseStatus());
  }

  useEffect(() => {
    void refresh();
  }, []);

  async function update(key: string, value: string) {
    setValues((current) => ({ ...current, [key]: value }));
    await setSetting(key, value);
  }

  const armed = (values.last_categories ?? "").split(",").filter(Boolean);

  return (
    <div className="flex h-full flex-col gap-8 overflow-auto p-8">
      <Section
        title="Pause"
        note="Taking a deliberate break is normal, and supported. Nothing is interrupted while a pause is running."
      >
        {pausedUntil ? (
          <div className="flex items-center gap-3">
            <p className="text-sm text-[var(--color-ink)]">
              Paused until {new Date(pausedUntil * 1000).toLocaleString()}
            </p>
            <Button
              onClick={async () => {
                await resumeNow();
                await refresh();
              }}
            >
              Resume now
            </Button>
          </div>
        ) : (
          <div className="flex gap-2">
            {[1, 3, 7].map((days) => (
              <Button
                key={days}
                onClick={async () => {
                  await pauseFor(days);
                  await refresh();
                }}
              >
                {days === 1 ? "A day" : days === 7 ? "A week" : `${days} days`}
              </Button>
            ))}
          </div>
        )}
      </Section>

      <Section
        title="What gets quieted"
        note="Your categories, not a verdict about the sites. A list someone else wrote gets ignored."
      >
        <div className="flex flex-wrap gap-2">
          {CATEGORIES.map((category) => {
            const on = armed.includes(category.id);
            return (
              <button
                key={category.id}
                type="button"
                title={category.detail}
                onClick={() =>
                  void update(
                    "last_categories",
                    (on
                      ? armed.filter((c) => c !== category.id)
                      : [...armed, category.id]
                    ).join(","),
                  )
                }
                className={clsx(
                  "rounded-full border px-4 py-2 text-sm transition-colors duration-150",
                  on
                    ? "border-[var(--color-accent)] text-[var(--color-ink)]"
                    : "border-[var(--color-border-subtle)] text-[var(--color-ink-muted)] hover:text-[var(--color-ink)]",
                )}
              >
                {category.label}
                <span className="ml-2 text-xs text-[var(--color-ink-muted)]">
                  {category.detail}
                </span>
              </button>
            );
          })}
        </div>
      </Section>

      <Section
        title="When it interrupts"
        note="Longer thresholds mean fewer interruptions. There is no right answer, only yours."
      >
        <Number
          label="Minutes locked before an unlock counts as returning"
          value={values.unlock_threshold_min ?? "20"}
          onChange={(v) => void update("unlock_threshold_min", v)}
        />
        <Number
          label="Minimum minutes between prompts"
          value={values.min_gap_min ?? "15"}
          onChange={(v) => void update("min_gap_min", v)}
        />
      </Section>

      <Section title="Browsers" note="">
        <p className="text-sm text-[var(--color-ink-muted)]">
          While a block is armed your browsers will say they are “managed by
          your organization”. That is Threshold turning off DNS-over-HTTPS —
          without it, blocking silently does nothing. It is removed the moment
          the block lifts.
        </p>
      </Section>
    </div>
  );
}

function Section({
  title,
  note,
  children,
}: {
  title: string;
  note: string;
  children: React.ReactNode;
}) {
  return (
    <section className="flex flex-col gap-3">
      <div>
        <h2 className="text-sm font-medium text-[var(--color-ink)]">{title}</h2>
        {note && (
          <p className="mt-1 max-w-prose text-xs text-[var(--color-ink-muted)]">
            {note}
          </p>
        )}
      </div>
      {children}
    </section>
  );
}

function Button({
  children,
  onClick,
}: {
  children: React.ReactNode;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="ritual-pressable rounded-full border border-[var(--color-border-subtle)] px-4 py-2 text-sm text-[var(--color-ink)]"
    >
      {children}
    </button>
  );
}

function Number({
  label,
  value,
  onChange,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
}) {
  return (
    <label className="flex items-center justify-between gap-4 text-sm">
      <span className="text-[var(--color-ink-muted)]">{label}</span>
      <input
        type="number"
        min={1}
        max={240}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        className="ritual-field w-24 rounded-full border border-[var(--color-border-subtle)] bg-white/[0.03] px-4 py-1.5 text-center outline-none focus:border-[color-mix(in_srgb,var(--color-accent)_60%,transparent)]"
      />
    </label>
  );
}
