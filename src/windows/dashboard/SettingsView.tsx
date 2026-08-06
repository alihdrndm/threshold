import { useEffect, useState } from "react";
import clsx from "clsx";
import {
  addQuote,
  chooseQuote,
  emergencyUnblock,
  getDiagnostics,
  getSettings,
  listQuotes,
  pauseFor,
  pauseStatus,
  rememberedSites,
  rememberSites,
  removeQuote,
  repairHelper,
  resumeNow,
  setSetting,
  type Diagnostics,
  type Quote,
  type QuoteSurface,
} from "@/lib/tauri";
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

/**
 * The blocklist's other half: sites you name yourself.
 *
 * Left-aligned and inline rather than centred like the ritual's version — this
 * is a settings row among settings rows, and borrowing the ritual's composure
 * here would make a list you edit look like a decision you are making.
 */
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
