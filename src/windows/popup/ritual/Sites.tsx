import { useState } from "react";
import { checkSite } from "@/lib/tauri";

/**
 * Sites the user names themselves, alongside the three built-in categories.
 *
 * The categories are somebody else's idea of what distracts you, and three of
 * them cannot be right for anyone in particular. This is where the blocklist
 * stops being generic — which is the whole reason the category framing works:
 * what drives real reduction is people classifying their own distractions.
 *
 * The typed name is resolved by the Rust side before it becomes a chip, so the
 * chip shows what will actually be blocked rather than an echo of the typing.
 * `https://www.Pinterest.com/pin/12` becomes `pinterest.com`, and a name that
 * cannot be blocked says so at the moment it is typed rather than failing later
 * inside a ritual nobody wants to repeat.
 */
export function Sites({
  sites,
  onChange,
}: {
  sites: string[];
  onChange: (next: string[]) => void;
}) {
  const [typed, setTyped] = useState("");
  const [problem, setProblem] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);

  async function add() {
    const candidate = typed.trim();
    if (!candidate || checking) return;

    setChecking(true);
    try {
      const host = await checkSite(candidate);
      // Already there is not an error — the chip below is the answer.
      if (!sites.includes(host)) onChange([...sites, host]);
      setTyped("");
      setProblem(null);
    } catch (reason) {
      setProblem(String(reason));
    } finally {
      setChecking(false);
    }
  }

  return (
    <div className="flex w-full flex-col items-center gap-2">
      {sites.length > 0 && (
        <div className="flex flex-wrap justify-center gap-2">
          {sites.map((site) => (
            <button
              key={site}
              type="button"
              onClick={() => onChange(sites.filter((s) => s !== site))}
              aria-label={`Stop blocking ${site}`}
              title="Remove"
              className="ritual-pressable flex items-center gap-2 rounded-full border border-[var(--color-accent)] bg-[color-mix(in_srgb,var(--color-accent)_12%,transparent)] px-4 py-2 text-sm text-[var(--color-ink)] focus-visible:outline-2 focus-visible:outline-offset-[3px] focus-visible:outline-[var(--color-accent)]"
            >
              {site}
              <span aria-hidden className="text-[var(--color-ink-muted)]">
                ×
              </span>
            </button>
          ))}
        </div>
      )}

      <input
        value={typed}
        onChange={(event) => {
          setTyped(event.target.value);
          // Clearing on the next keystroke: a refusal that outlives the thing it
          // refused reads as the field being broken.
          if (problem) setProblem(null);
        }}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            void add();
          }
        }}
        // Typing a site and walking to the Start button should not silently drop
        // it, and this screen has no other submit for the field.
        onBlur={() => void add()}
        placeholder="Add a site — pinterest.com"
        spellCheck={false}
        autoCapitalize="off"
        autoCorrect="off"
        className="ritual-field w-64 rounded-full border border-[var(--color-border-subtle)] bg-[var(--color-fill-subtle)] px-5 py-2 text-center text-sm text-[var(--color-ink)] outline-none placeholder:text-[var(--color-ink-muted)] focus:border-[color-mix(in_srgb,var(--color-accent)_65%,transparent)]"
      />

      {problem && (
        <p role="alert" className="text-center text-xs text-[var(--color-ink-muted)]">
          {problem}
        </p>
      )}
    </div>
  );
}
