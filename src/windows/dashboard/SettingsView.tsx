import { useEffect, useState } from "react";
import clsx from "clsx";
import {
  addContext,
  addQuote,
  calendarStatus,
  calendarSyncNow,
  googleConnect,
  googleDisconnect,
  chooseQuote,
  clearHistory,
  emergencyUnblock,
  getDiagnostics,
  getSettings,
  listContexts,
  listQuotes,
  pauseFor,
  pauseStatus,
  rememberedSites,
  rememberSites,
  removeContext,
  removeQuote,
  renameContext,
  repairHelper,
  resumeNow,
  setSetting,
  type Diagnostics,
  type CalendarStatus,
  type Quote,
  type QuoteSurface,
  type TaskContext,
} from "@/lib/tauri";
import { listen } from "@tauri-apps/api/event";
import { useSiteEditor } from "@/sites/useSiteEditor";
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
  // Armed, then confirmed: the one action here no undo can forgive, so it is
  // the one place a second click is asked for instead.
  const [clearArmed, setClearArmed] = useState(false);
  const [cleared, setCleared] = useState<string | null>(null);
  const [sites, setSites] = useState<string[]>([]);
  const [quotes, setQuotes] = useState<Quote[]>([]);

  async function refresh() {
    const pairs = await getSettings();
    setValues(Object.fromEntries(pairs));
    setPausedUntil(await pauseStatus());
    setDiagnostics(await getDiagnostics().catch(() => null));
    setSites(await rememberedSites().catch(() => []));
    setQuotes(await listQuotes().catch(() => []));
  }

  // Saved as it changes rather than behind a Save button: everything else on
  // this screen already works that way, and a list that needed confirming would
  // be the one thing here you could lose by closing the window.
  //
  // The stored list is what comes back, not what was sent — Rust tidies and
  // de-duplicates, and the chips should show what is actually kept.
  async function saveSites(next: string[]) {
    setSites(next);
    setSites(await rememberSites(next).catch(() => next));
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
        note="Your categories, not a verdict about the sites. A list someone else wrote gets ignored. Sites you add here are the same list the ritual offers, and changing it does not alter a block already running — that one holds to what it committed to."
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

        <SiteList sites={sites} onChange={saveSites} />
      </Section>

      {/* Placed after the blocklist because that is where it is read: the two
          moments a quote appears are the ritual and the wall a blocked site
          puts up. */}
      <Section
        title="Your quotes"
        note="Words worth reading at the moment an urge arrives. Nothing else in this app tries to motivate you — praise for setting an intention licenses the very thing you were avoiding — but a line you chose yourself is not this program talking, and that is a different matter. Leave it empty and no quote appears anywhere."
      >
        <QuoteReservoir
          quotes={quotes}
          onChange={setQuotes}
          ritual={values.quote_ritual ?? "shuffle"}
          blocked={values.quote_blocked ?? "shuffle"}
          onChoose={async (surface, id) => {
            await chooseQuote(surface, id);
            await refresh();
          }}
        />
      </Section>

      <Section
        title="Areas"
        note="The one home a task has — Job, Personal, Side, or whatever your life is actually divided into. Type #name when adding a task, or drop a card on an area on the Tasks tab. Removing an area leaves its tasks where they are, unlabelled. Eight at most: past that they stop being areas and start being tags."
      >
        <AreaList />
      </Section>

      <Section
        title="Working hours"
        note="When a task dropped into Schedule may be booked. Threshold picks the soonest free gap inside these hours, on these days, keeping a buffer around whatever is already on your Google Calendar."
      >
        <WorkingHours values={values} update={update} />
      </Section>

      <Section
        title="Google Calendar"
        note="Connect a calendar and a task dropped into Schedule gets a 30-minute event at the next free slot — yours to move here or in Google. Threshold uses your own Google credentials; nothing is shared with anyone else."
      >
        <GoogleCalendar />
      </Section>

      {/* Seconds, not minutes: the people who want every return met want
          ten seconds, and a field that could not say ten seconds was a
          field that could not say what they meant. Zero is allowed and means
          "always". Values set in the old minute fields are shown converted,
          so nothing anyone chose is lost in the change of unit. */}
      <Section
        title="When it interrupts"
        note="Longer thresholds mean fewer interruptions. There is no right answer, only yours — and it takes effect on the next return, no restart needed."
      >
        <Number
          label="Seconds locked before an unlock counts as returning"
          unit="s"
          value={values.unlock_threshold_sec ?? seconds(values.unlock_threshold_min, 20 * 60)}
          onChange={(v) => void update("unlock_threshold_sec", v)}
        />
        <Number
          label="Minimum seconds between prompts"
          unit="s"
          value={values.min_gap_sec ?? seconds(values.min_gap_min, 15 * 60)}
          onChange={(v) => void update("min_gap_sec", v)}
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
          your organization”. That is Threshold turning off DNS-over-HTTPS —
          without it, blocking silently does nothing. It is removed when the
          block lifts.
        </p>
        <p className="max-w-prose text-sm text-[var(--color-ink-muted)]">
          A blocked site fails with a plain connection error, and Threshold
          answers in the middle of the screen with a quote you chose — the
          attempt lands on an address this app is listening at, which is how it
          knows an urge arrived. One honest caveat: a site you visited moments
          before the block may keep working for up to a minute, and the reverse
          after it lifts. Browsers remember addresses briefly, and that memory
          belongs to them.
        </p>
      </Section>

      {/* The chime is on by default and yours to turn off. Same shape as the
          appearance control: a choice between named states, not a switch
          whose "on" side you have to guess. */}
      <Section
        title="Sound at the wall"
        note="Two soft notes when the quote appears, so it is heard as well as seen. Off keeps the quote and drops the sound."
      >
        <div role="radiogroup" aria-label="Sound at the wall" className="flex flex-wrap gap-2">
          {(["on", "off"] as const).map((option) => {
            const on = (values.wall_sound ?? "on") === option;
            return (
              <button
                key={option}
                type="button"
                role="radio"
                aria-checked={on}
                onClick={() => void update("wall_sound", option)}
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

      {/* Everywhere else this app prefers undo to confirmation - but there is
          no undo for forgetting, so this one action asks twice. Neutral
          colours on both buttons: red here would only teach the eye that red
          means "the thing I clicked on purpose". */}
      <Section
        title="Start over"
        note="This erases the record the Overview is built from — every session and every intention, for good. Tasks, quotes, sites and settings stay. There is no undo."
      >
        {clearArmed ? (
          <div className="flex items-center gap-3">
            <Button
              onClick={async () => {
                try {
                  await clearHistory();
                  setCleared("Cleared. The record starts now.");
                } catch (err) {
                  setCleared(err instanceof Error ? err.message : String(err));
                }
                setClearArmed(false);
              }}
            >
              Yes — erase it all
            </Button>
            <Button onClick={() => setClearArmed(false)}>Keep it</Button>
          </div>
        ) : (
          <div className="flex items-center gap-3">
            <Button
              onClick={() => {
                setCleared(null);
                setClearArmed(true);
              }}
            >
              Clear the record…
            </Button>
            {cleared && (
              <span className="text-xs text-[var(--color-ink-muted)]">
                {cleared}
              </span>
            )}
          </div>
        )}
      </Section>
    </div>
  );
}

/**
 * The blocklist's other half: sites you name yourself.
 *
 * Left-aligned and inline rather than centred like the ritual's version — this
 * is a settings row among settings rows, and borrowing the ritual's composure
 * here would make a list you edit look like a decision you are making.
 */
/** Seven day toggles + open/close times + buffer, saved as they change. */
function WorkingHours({
  values,
  update,
}: {
  values: Record<string, string>;
  update: (key: string, value: string) => void;
}) {
  const days = (values.work_days ?? "1,2,3,4,5")
    .split(",")
    .map((n) => n.trim())
    .filter(Boolean);
  const labels = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

  function toggleDay(n: number) {
    const set = new Set(days);
    const key = String(n);
    if (set.has(key)) set.delete(key);
    else set.add(key);
    const next = [...set]
      .map((x) => parseInt(x, 10))
      .sort((a, b) => a - b)
      .join(",");
    update("work_days", next);
  }

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap gap-2">
        {labels.map((label, i) => {
          const n = i + 1;
          const on = days.includes(String(n));
          return (
            <button
              key={label}
              type="button"
              aria-pressed={on}
              onClick={() => toggleDay(n)}
              className={clsx(
                "ritual-pressable rounded-full border px-3 py-1.5 text-xs",
                on
                  ? "border-[var(--color-accent)] text-[var(--color-ink)]"
                  : "border-[var(--color-border-subtle)] text-[var(--color-ink-muted)] hover:text-[var(--color-ink)]",
              )}
            >
              {label}
            </button>
          );
        })}
      </div>
      <div className="flex flex-wrap gap-6">
        <TimeField
          label="Opens"
          value={values.work_start ?? "09:00"}
          onChange={(v) => update("work_start", v)}
        />
        <TimeField
          label="Closes"
          value={values.work_end ?? "18:00"}
          onChange={(v) => update("work_end", v)}
        />
        <Number
          label="Buffer around events"
          unit="min"
          value={values.cal_buffer_min ?? "15"}
          onChange={(v) => update("cal_buffer_min", v)}
        />
      </div>
    </div>
  );
}

function TimeField({
  label,
  value,
  onChange,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
}) {
  return (
    <label className="flex items-center gap-3 text-sm">
      <span className="text-[var(--color-ink-muted)]">{label}</span>
      <input
        type="time"
        value={value}
        onChange={(event) => onChange(event.target.value)}
        className="ritual-field rounded-full border border-[var(--color-border-subtle)] bg-[var(--color-fill-subtle)] px-4 py-1.5 text-center text-[var(--color-ink)] outline-none focus:border-[color-mix(in_srgb,var(--color-accent)_60%,transparent)]"
      />
    </label>
  );
}

/**
 * Connect Google, or set it up. The client id and optional secret are the
 * user's own - the app ships none - so the section carries the steps to make
 * one, collapsed until asked for.
 */
function GoogleCalendar() {
  const [status, setStatus] = useState<CalendarStatus | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [id, setId] = useState("");
  const [secret, setSecret] = useState("");
  const [showHelp, setShowHelp] = useState(false);

  async function refresh() {
    setStatus(await calendarStatus().catch(() => null));
  }
  useEffect(() => {
    void refresh();
    const unlisten = listen("calendar-status", () => void refresh());
    return () => void unlisten.then((off) => off());
  }, []);

  async function saveCreds() {
    await setSetting("google_client_id", id.trim());
    await setSetting("google_client_secret", secret.trim());
  }

  const connected = status?.connected ?? false;

  return (
    <div className="flex flex-col gap-4">
      {connected ? (
        <div className="flex flex-wrap items-center gap-3">
          <span className="rounded-full border border-[var(--color-accent)] px-3 py-1.5 text-xs text-[var(--color-ink)]">
            Connected
          </span>
          <Button
            onClick={async () => {
              setBusy("Syncing…");
              const msg = await calendarSyncNow().catch((e) =>
                e instanceof Error ? e.message : String(e),
              );
              setBusy(msg);
              await refresh();
            }}
          >
            Sync now
          </Button>
          <Button
            onClick={async () => {
              await googleDisconnect();
              await refresh();
            }}
          >
            Disconnect
          </Button>
          {busy && (
            <span className="text-xs text-[var(--color-ink-muted)]">{busy}</span>
          )}
        </div>
      ) : (
        <div className="flex flex-col gap-3">
          <label className="flex flex-col gap-1 text-sm">
            <span className="text-[var(--color-ink-muted)]">Client ID</span>
            <input
              value={id}
              onChange={(event) => setId(event.target.value)}
              placeholder="…apps.googleusercontent.com"
              className="ritual-field w-full rounded-xl border border-[var(--color-border-subtle)] bg-[var(--color-fill-subtle)] px-4 py-2 text-sm text-[var(--color-ink)] outline-none focus:border-[color-mix(in_srgb,var(--color-accent)_60%,transparent)]"
            />
          </label>
          <label className="flex flex-col gap-1 text-sm">
            <span className="text-[var(--color-ink-muted)]">
              Client secret (optional)
            </span>
            <input
              value={secret}
              onChange={(event) => setSecret(event.target.value)}
              className="ritual-field w-full rounded-xl border border-[var(--color-border-subtle)] bg-[var(--color-fill-subtle)] px-4 py-2 text-sm text-[var(--color-ink)] outline-none focus:border-[color-mix(in_srgb,var(--color-accent)_60%,transparent)]"
            />
          </label>
          <div className="flex flex-wrap items-center gap-3">
            <Button
              onClick={async () => {
                if (!id.trim()) {
                  setBusy("Paste your Client ID first.");
                  return;
                }
                setBusy("Opening your browser…");
                await saveCreds();
                try {
                  await googleConnect();
                  setBusy(null);
                } catch (e) {
                  setBusy(e instanceof Error ? e.message : String(e));
                }
                await refresh();
              }}
            >
              Connect
            </Button>
            <button
              type="button"
              onClick={() => setShowHelp((h) => !h)}
              className="text-xs text-[var(--color-ink-muted)] underline underline-offset-2 hover:text-[var(--color-ink)]"
            >
              How to get a Client ID
            </button>
            {busy && (
              <span className="text-xs text-[var(--color-ink-muted)]">{busy}</span>
            )}
          </div>
          {showHelp && (
            <ol className="flex list-decimal flex-col gap-1 rounded-xl border border-[var(--color-border-subtle)] bg-[var(--color-fill-subtle)] p-4 pl-8 text-xs text-[var(--color-ink-muted)]">
              <li>
                Open{" "}
                <button
                  type="button"
                  onClick={() => void openHelp()}
                  className="text-[var(--color-ink)] underline underline-offset-2"
                >
                  console.cloud.google.com
                </button>{" "}
                and make a project.
              </li>
              <li>APIs &amp; Services &rarr; Library &rarr; enable Google Calendar API.</li>
              <li>
                OAuth consent screen &rarr; External &rarr; fill the basics &rarr; Publish
                app (choose In production; Testing expires the link every 7 days).
              </li>
              <li>Credentials &rarr; Create credentials &rarr; OAuth client ID.</li>
              <li>Application type &rarr; Desktop app.</li>
              <li>Copy the Client ID here and press Connect.</li>
            </ol>
          )}
        </div>
      )}
      {status?.lastSyncStatus && (
        <p className="text-xs text-[var(--color-ink-muted)]">
          Last sync: {status.lastSyncStatus}
          {status.lastSyncTs
            ? ` · ${new Date(status.lastSyncTs * 1000).toLocaleString()}`
            : ""}
        </p>
      )}
    </div>
  );
}

async function openHelp() {
  const { openUrl } = await import("@/lib/tauri");
  await openUrl("https://console.cloud.google.com/");
}

/**
 * The areas, editable in place. Click a name to rename it; Enter keeps,
 * Escape forgets. The × removes the area and only the area - its tasks stay,
 * unlabelled - which is why there is no "are you sure" here either.
 */
function AreaList() {
  const [areas, setAreas] = useState<TaskContext[]>([]);
  const [editing, setEditing] = useState<{ id: number; name: string } | null>(
    null,
  );
  const [adding, setAdding] = useState("");
  const [problem, setProblem] = useState<string | null>(null);

  useEffect(() => {
    listContexts()
      .then(setAreas)
      .catch(() => setAreas([]));
  }, []);

  function attempt(action: () => Promise<TaskContext[]>) {
    setProblem(null);
    action()
      .then(setAreas)
      .catch((err: unknown) =>
        setProblem(err instanceof Error ? err.message : String(err)),
      );
  }

  return (
    <div className="flex flex-col gap-3">
      <ul className="flex flex-wrap gap-2">
        {areas.map((area) => (
          <li
            key={area.id}
            className="flex items-center gap-1 rounded-full border border-[var(--color-border-subtle)] pr-1 pl-3 text-sm"
          >
            {editing?.id === area.id ? (
              <input
                autoFocus
                value={editing.name}
                onChange={(event) =>
                  setEditing({ id: area.id, name: event.target.value })
                }
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    const name = editing.name;
                    setEditing(null);
                    attempt(() => renameContext(area.id, name));
                  }
                  if (event.key === "Escape") setEditing(null);
                }}
                onBlur={() => setEditing(null)}
                aria-label={`Rename ${area.name}`}
                className="w-28 bg-transparent py-1.5 text-sm text-[var(--color-ink)] outline-none"
              />
            ) : (
              <button
                type="button"
                onClick={() => setEditing({ id: area.id, name: area.name })}
                title="Rename"
                className="py-1.5 text-[var(--color-ink)]"
              >
                {area.name}
              </button>
            )}
            <button
              type="button"
              onClick={() => attempt(() => removeContext(area.id))}
              aria-label={`Remove ${area.name}`}
              className="grid size-6 place-items-center rounded-full text-[var(--color-ink-muted)] transition duration-150 hover:bg-[var(--color-fill-selected)] hover:text-[var(--color-ink)] active:scale-90"
            >
              ×
            </button>
          </li>
        ))}
        <li>
          <input
            value={adding}
            onChange={(event) => setAdding(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && adding.trim()) {
                const name = adding;
                setAdding("");
                attempt(() => addContext(name));
              }
            }}
            placeholder="New area"
            aria-label="New area"
            className="ritual-field w-36 rounded-full border border-[var(--color-border-subtle)] bg-[var(--color-fill-subtle)] px-3 py-1.5 text-sm text-[var(--color-ink)] outline-none placeholder:text-[var(--color-ink-muted)] focus:border-[color-mix(in_srgb,var(--color-accent)_60%,transparent)]"
          />
        </li>
      </ul>
      {problem && (
        <p className="text-xs text-[var(--color-ink-muted)]">{problem}</p>
      )}
    </div>
  );
}

function SiteList({
  sites,
  onChange,
}: {
  sites: string[];
  onChange: (next: string[]) => void;
}) {
  const editor = useSiteEditor(sites, onChange);

  return (
    <div className="flex flex-col gap-2">
      {sites.length > 0 && (
        <div className="flex flex-wrap gap-2">
          {sites.map((site) => (
            <button
              key={site}
              type="button"
              onClick={() => editor.remove(site)}
              aria-label={`Stop blocking ${site}`}
              title="Remove"
              className="ritual-pressable flex items-center gap-2 rounded-full border border-[var(--color-accent)] px-4 py-2 text-sm text-[var(--color-ink)]"
            >
              {site}
              <span aria-hidden className="text-[var(--color-ink-muted)]">
                ×
              </span>
            </button>
          ))}
        </div>
      )}

      <div className="flex items-center gap-2">
        <input
          value={editor.typed}
          onChange={(event) => editor.onType(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              void editor.add();
            }
          }}
          placeholder="Add a site — pinterest.com"
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
          aria-label="Add a site to block"
          className="ritual-field w-64 rounded-full border border-[var(--color-border-subtle)] bg-[var(--color-fill-subtle)] px-4 py-2 text-sm text-[var(--color-ink)] outline-none placeholder:text-[var(--color-ink-muted)] focus:border-[color-mix(in_srgb,var(--color-accent)_60%,transparent)]"
        />
        <Button onClick={() => void editor.add()}>Add</Button>
      </div>

      {editor.problem && (
        <p role="alert" className="text-xs text-[var(--color-ink-muted)]">
          {editor.problem}
        </p>
      )}
    </div>
  );
}

/**
 * The quote reservoir, and which quote each surface shows.
 *
 * "Shuffle" is the default and is listed first, because a fixed line habituates
 * exactly as fast as a fixed dialog does — the same finding that makes the
 * ritual rotate its phrasing daily. Pinning is there for the person who has one
 * line that actually works on them, which is a real thing and worth allowing.
 */
function QuoteReservoir({
  quotes,
  onChange,
  ritual,
  blocked,
  onChoose,
}: {
  quotes: Quote[];
  onChange: (next: Quote[]) => void;
  ritual: string;
  blocked: string;
  onChoose: (surface: QuoteSurface, id: number | null) => Promise<void>;
}) {
  const [text, setText] = useState("");
  const [author, setAuthor] = useState("");
  const [problem, setProblem] = useState<string | null>(null);

  async function add() {
    if (!text.trim()) return;
    try {
      onChange(await addQuote(text, author.trim() || null));
      setText("");
      setAuthor("");
      setProblem(null);
    } catch (reason) {
      setProblem(String(reason));
    }
  }

  return (
    <div className="flex flex-col gap-4">
      {quotes.length > 0 && (
        <ul className="flex flex-col gap-2">
          {quotes.map((quote) => (
            <li
              key={quote.id}
              className="flex items-start justify-between gap-4 rounded-2xl border border-[var(--color-border-subtle)] px-4 py-3"
            >
              <div className="flex flex-col gap-1">
                <p className="text-sm text-[var(--color-ink)]">{quote.text}</p>
                {quote.author && (
                  <p className="text-xs text-[var(--color-ink-muted)]">
                    {quote.author}
                  </p>
                )}
              </div>
              <button
                type="button"
                onClick={async () => onChange(await removeQuote(quote.id))}
                aria-label="Remove this quote"
                title="Remove"
                className="ritual-pressable shrink-0 rounded-full px-2 text-sm text-[var(--color-ink-muted)]"
              >
                ×
              </button>
            </li>
          ))}
        </ul>
      )}

      <div className="flex flex-col gap-2">
        <textarea
          value={text}
          onChange={(event) => {
            setText(event.target.value);
            if (problem) setProblem(null);
          }}
          rows={2}
          placeholder="A line worth reading when the urge arrives"
          className="ritual-field w-full resize-none rounded-2xl border border-[var(--color-border-subtle)] bg-[var(--color-fill-subtle)] px-4 py-3 text-sm text-[var(--color-ink)] outline-none placeholder:text-[var(--color-ink-muted)] focus:border-[color-mix(in_srgb,var(--color-accent)_60%,transparent)]"
        />
        <div className="flex items-center gap-2">
          <input
            value={author}
            onChange={(event) => setAuthor(event.target.value)}
            placeholder="Who said it — optional"
            aria-label="Author, optional"
            className="ritual-field w-64 rounded-full border border-[var(--color-border-subtle)] bg-[var(--color-fill-subtle)] px-4 py-2 text-sm text-[var(--color-ink)] outline-none placeholder:text-[var(--color-ink-muted)] focus:border-[color-mix(in_srgb,var(--color-accent)_60%,transparent)]"
          />
          <Button onClick={() => void add()}>Keep it</Button>
        </div>
        {problem && (
          <p role="alert" className="text-xs text-[var(--color-ink-muted)]">
            {problem}
          </p>
        )}
      </div>

      {quotes.length > 0 && (
        <div className="flex flex-col gap-3">
          <QuotePicker
            label="In the ritual"
            quotes={quotes}
            value={ritual}
            onChoose={(id) => onChoose("ritual", id)}
          />
          <QuotePicker
            label="On a blocked site"
            quotes={quotes}
            value={blocked}
            onChoose={(id) => onChoose("blocked", id)}
          />
        </div>
      )}
    </div>
  );
}

function QuotePicker({
  label,
  quotes,
  value,
  onChoose,
}: {
  label: string;
  quotes: Quote[];
  value: string;
  onChoose: (id: number | null) => Promise<void>;
}) {
  // A quote pinned and then deleted leaves a stale id here; Rust already falls
  // back to shuffling, so the control agrees with what actually happens.
  const known = quotes.some((quote) => String(quote.id) === value);
  const selected = known ? value : "shuffle";

  return (
    <label className="flex items-center justify-between gap-4 text-sm">
      <span className="text-[var(--color-ink-muted)]">{label}</span>
      <select
        value={selected}
        onChange={(event) =>
          void onChoose(
            // `parseInt`, not `Number` — this file has a `Number` settings
            // component that shadows the global.
            event.target.value === "shuffle"
              ? null
              : parseInt(event.target.value, 10),
          )
        }
        className="ritual-field max-w-xs truncate rounded-full border border-[var(--color-border-subtle)] bg-[var(--color-fill-subtle)] px-4 py-2 text-sm text-[var(--color-ink)] outline-none"
      >
        <option value="shuffle">Shuffle them</option>
        {quotes.map((quote) => (
          <option key={quote.id} value={quote.id}>
            {quote.text.length > 48
              ? `${quote.text.slice(0, 48)}…`
              : quote.text}
          </option>
        ))}
      </select>
    </label>
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

/** An old minute value as seconds, or the default when there is none. */
function seconds(minutes: string | undefined, fallback: number): string {
  const n = window.Number(minutes);
  return minutes !== undefined && window.Number.isFinite(n) && n >= 0
    ? String(Math.round(n * 60))
    : String(fallback);
}

function Number({
  label,
  value,
  unit,
  onChange,
}: {
  label: string;
  value: string;
  /** Shown after the field, so the number is never read without its unit. */
  unit?: string;
  onChange: (value: string) => void;
}) {
  return (
    <label className="flex items-center justify-between gap-4 text-sm">
      <span className="text-[var(--color-ink-muted)]">{label}</span>
      <span className="flex items-center gap-2">
        <input
          type="number"
          min={0}
          max={86_400}
          step={1}
          value={value}
          onChange={(event) => onChange(event.target.value)}
          className="ritual-field w-28 rounded-full border border-[var(--color-border-subtle)] bg-[var(--color-fill-subtle)] px-4 py-1.5 text-center outline-none focus:border-[color-mix(in_srgb,var(--color-accent)_60%,transparent)]"
        />
        {unit && (
          <span className="w-3 text-xs text-[var(--color-ink-muted)]">{unit}</span>
        )}
      </span>
    </label>
  );
}
