import { useSiteEditor } from "@/sites/useSiteEditor";

/**
 * Sites the user names themselves, alongside the three built-in categories.
 *
 * The categories are somebody else's idea of what distracts you, and three of
 * them cannot be right for anyone in particular. This is where the blocklist
 * stops being generic — which is the whole reason the category framing works:
 * what drives real reduction is people classifying their own distractions.
 *
 * Same list Settings edits; whichever screen you are on, it is the one list.
 */
/**
 * How many chips this screen will show before summarising the rest.
 *
 * The ritual is fullscreen and must never scroll — the express path already
 * fills a 768px display with seven rows, and chips wrapping to a third line
 * would push the last row into the exit at the bottom. Four is what fits.
 *
 * Truncating information at the moment of commitment is not free, so what is
 * hidden is still counted rather than silently dropped, and Settings — which
 * scrolls — shows the whole list. That is the right division anyway: the ritual
 * is where you commit, Settings is where you keep house.
 */
const CHIPS_ON_SCREEN = 4;

export function Sites({
  sites,
  onChange,
}: {
  sites: string[];
  onChange: (next: string[]) => void;
}) {
  const editor = useSiteEditor(sites, onChange);
  const shown = sites.slice(0, CHIPS_ON_SCREEN);
  const hidden = sites.length - shown.length;

  return (
    <div className="flex w-full flex-col items-center gap-2">
      {sites.length > 0 && (
        <div className="flex flex-wrap items-center justify-center gap-2">
          {shown.map((site) => (
            <button
              key={site}
              type="button"
              onClick={() => editor.remove(site)}
              aria-label={`Stop blocking ${site}`}
              title="Remove"
              className="ritual-pressable flex items-center gap-1.5 rounded-full border border-[var(--color-accent)] bg-[color-mix(in_srgb,var(--color-accent)_12%,transparent)] px-3 py-1 text-xs text-[var(--color-ink)] focus-visible:outline-2 focus-visible:outline-offset-[3px] focus-visible:outline-[var(--color-accent)]"
            >
              {site}
              <span aria-hidden className="text-[var(--color-ink-muted)]">
                ×
              </span>
            </button>
          ))}
          {hidden > 0 && (
            <span className="text-xs text-[var(--color-ink-muted)]">
              and {hidden} more
            </span>
          )}
        </div>
      )}

      <input
        value={editor.typed}
        onChange={(event) => editor.onType(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            void editor.add();
          }
        }}
        // Typing a site and walking to the Start button should not silently drop
        // it, and this screen has no other submit for the field.
        onBlur={() => void editor.add()}
        placeholder="Add a site — pinterest.com"
        spellCheck={false}
        autoCapitalize="off"
        autoCorrect="off"
        className="ritual-field w-64 rounded-full border border-[var(--color-border-subtle)] bg-[var(--color-fill-subtle)] px-5 py-2 text-center text-sm text-[var(--color-ink)] outline-none placeholder:text-[var(--color-ink-muted)] focus:border-[color-mix(in_srgb,var(--color-accent)_65%,transparent)]"
      />

      {editor.problem && (
        <p role="alert" className="text-center text-xs text-[var(--color-ink-muted)]">
          {editor.problem}
        </p>
      )}
    </div>
  );
}
