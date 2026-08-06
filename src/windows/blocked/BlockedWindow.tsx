import { useEffect, useState } from "react";
import { quoteFor, type Quote } from "@/lib/tauri";

/**
 * The wall a blocked site puts up, answered in your own words.
 *
 * The browser's own error page cannot be changed — no policy exists for it, and
 * replacing it would need a locally trusted certificate authority, which is far
 * too high a price for decorating a failure. So the page stays the browser's and
 * this stands beside it.
 *
 * It carries a quote and nothing else. No counter, no "you have been blocked",
 * no name of the site that was reached for: foregrounding what you are avoiding
 * makes it more available rather than less, which is the same reason the session
 * banner names the task and never the blocklist. A tally would be worse still —
 * a number here is a scoreboard for the wrong thing.
 *
 * With an empty reservoir this renders nothing, and the window shows an empty
 * frame for a moment before removing itself. That is the honest outcome: a line
 * the app chose for you is exactly the borrowed sentiment the reservoir exists
 * to replace.
 */
export function BlockedWindow() {
  const [quote, setQuote] = useState<Quote | null>(null);

  useEffect(() => {
    quoteFor("blocked")
      .then(setQuote)
      .catch(() => setQuote(null));
  }, []);

  if (!quote) return null;

  return (
    <main className="checkin-card flex h-screen w-screen flex-col justify-center gap-3 px-7">
      <blockquote className="flex flex-col gap-2">
        <p className="text-lg leading-relaxed font-light text-balance text-[var(--color-ink)]">
          {quote.text}
        </p>
        {quote.author && (
          <cite className="text-xs tracking-wide text-[var(--color-ink-muted)] not-italic">
            {quote.author}
          </cite>
        )}
      </blockquote>
    </main>
  );
}
