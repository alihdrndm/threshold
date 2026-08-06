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
export function Sites({
  sites,
  onChange,
}: {
  sites: string[];
  onChange: (next: string[]) => void;
}) {
  const editor = useSiteEditor(sites, onChange);

  return (
    <div className="flex w-full flex-col items-center gap-2">
      {sites.length > 0 && (
        <div className="flex flex-wrap justify-center gap-2">
          {sites.map((site) => (
            <button
              key={site}
              type="button"
              onClick={() => editor.remove(site)}
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
