import { useEffect, useState } from "react";
import clsx from "clsx";
import {
  emergencyUnblock,
  getDiagnostics,
  getSettings,
  pauseFor,
  pauseStatus,
  repairHelper,
  resumeNow,
  setSetting,
  type Diagnostics,
} from "@/lib/tauri";
import { CATEGORIES } from "../popup/ritual/copy";
import type { Appearance } from "@/appearance";

/**
 * Settings (spec F5).
 *
 * Everything here is editable on purpose. Self-set limits are the ones people
 * keep - imposed ones get around 25% compliance and breed workarounds - so the
 * categories, the thresholds and the pause are all yours to change.
 */
export function SettingsView({
  appearance,
  onAppearanceChange,
}: {
  appearance: Appearance;
  onAppearanceChange: (next: Appearance) => void;
}) {
  const [values, setValues] = useState<Record<string, string>>({});
  const [pausedUntil, setPausedUntil] = useState<number | null>(null);
  const [diagnostics, setDiagnostics] = useState<Diagnostics | null>(null);
  const [repair, setRepair] = useState<string | null>(null);
  const [unblock, setUnblock] = useState<string | null>(null);

  async function refresh() {
    const pairs = await getSettings();
    setValues(Object.fromEntries(pairs));
    setPausedUntil(await pauseStatus());
    setDiagnostics(await getDiagnostics().catch(() => null));
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

      <Section
        title="Appearance"
        note="Light, dark, or whatever Windows is doing. The ritual stays dark either way — a white fullscreen window at boot is not a kindness, and its daily rotation is dark by design."
      >
        <div
          role="radiogroup"
          aria-label="Appearance"
          className="flex flex-wrap gap-2"
        >
          {(["system", "light", "dark"] as const).map((option) => {
            const on = appearance === option;
            return (
              <button
                key={option}
                type="button"
                role="radio"
                aria-checked={on}
                onClick={() => onAppearanceChange(option)}
                className={clsx(
                  "ritual-pressable rounded-full border px-4 py-2 text-sm capitalize",
                  on
                    ? "border-[var(--color-accent)] text-[var(--color-ink)]"
                    : "border-[var(--color-border-subtle)] text-[var(--color-ink-muted)] hover:text-[var(--color-ink)]",
                )}
              >
                {option}
              </button>
            );
          })}
        </div>
      </Section>

      <Section
        title="Setup"
        note="Whether this installation can actually do its job. Blocking needs a helper registered with administrator rights, and the triggers need scheduled tasks pointing at this build."
      >
        {diagnostics ? (
          <div className="flex flex-col gap-2">
            {diagnostics.checks.map((check) => (
              <div
                key={check.name}
                className="flex items-baseline gap-3 rounded-lg border border-[var(--color-border-subtle)] px-3 py-2 text-sm"
              >
                <span
                  className={
                    check.ok
                      ? "text-[var(--color-accent)]"
                      : "text-[var(--color-ink)]"
                  }
                >
                  {check.ok ? "OK" : "!"}
                </span>
                <span className="w-48 shrink-0">{check.name}</span>
                <span className="flex-1 text-xs break-all text-[var(--color-ink-muted)]">
                  {check.detail}
                </span>
              </div>
            ))}

            {diagnostics.needsRepair && (
              <div className="mt-2 flex flex-col gap-3 rounded-xl border border-[var(--color-accent)]/50 bg-[var(--color-accent)]/[0.08] p-4">
                <p className="text-sm">
                  Blocking cannot work until the helper is registered. This needs
                  administrator rights once.
                </p>
                <div className="flex items-center gap-3">
                  <Button
                    onClick={async () => {
                      setRepair("Waiting for administrator rights…");
                      try {
                        await repairHelper();
                        setRepair("Done — blocking is ready.");
                      } catch (err) {
                        setRepair(
                          err instanceof Error ? err.message : String(err),
                        );
                      }
                      await refresh();
                    }}
                  >
                    Repair
                  </Button>
                  {repair && (
                    <span className="text-xs text-[var(--color-ink-muted)]">
                      {repair}
                    </span>
                  )}
                </div>
              </div>
            )}
          </div>
        ) : (
          <p className="text-sm text-[var(--color-ink-muted)]">Checking…</p>
        )}
      </Section>

      <Section title="Browsers" note="">
        <p className="max-w-prose text-sm text-[var(--color-ink-muted)]">
          While a block is armed your browsers will say they are “managed by
          your organization”, and a blocked site shows the browser’s own
          “blocked by your administrator” page. That is Threshold: it turns off
          DNS-over-HTTPS and adds the sites to the browser’s blocklist. Both are
          removed when the block lifts.
        </p>
        <p className="max-w-prose text-sm text-[var(--color-ink-muted)]">
          The browser policy is what does the real work. Blocking by address
          alone sits underneath the browser, and a site that keeps an offline
          copy of itself — x.com is one — answers from that copy before any
          address is looked up.
        </p>
      </Section>

      {/* Stated plainly rather than buried. A commitment you cannot leave is a
          trap, and a trap is what people uninstall. Knowing the door is there
          is most of what makes staying a choice. */}
      <Section
        title="If you need out"
        note="A block holds until its time is up, even against a restart or a changed clock. This ends one early. It is recorded as a count — not a note, not a judgement — because that is the only honest way to keep an exit that people will actually use."
      >
        <div className="flex items-center gap-3">
          <Button
            onClick={async () => {
              setUnblock("Asking the helper…");
              try {
                await emergencyUnblock();
                // Only reached once the hosts file has been read back clear
                // and the browser policy is confirmed gone. It used to be said
                // unconditionally, which is how "unblocked" and "still blocked"
                // came to look identical from here.
                setUnblock(
                  "Done — the sites are open again. A tab left open may need a reload.",
                );
              } catch (err) {
                setUnblock(err instanceof Error ? err.message : String(err));
              }
              await refresh();
            }}
          >
            End the block now
          </Button>
          {unblock && (
            <span className="text-xs text-[var(--color-ink-muted)]">
              {unblock}
            </span>
          )}
        </div>
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
        className="ritual-field w-24 rounded-full border border-[var(--color-border-subtle)] bg-[var(--color-fill-subtle)] px-4 py-1.5 text-center outline-none focus:border-[color-mix(in_srgb,var(--color-accent)_60%,transparent)]"
      />
    </label>
  );
}
