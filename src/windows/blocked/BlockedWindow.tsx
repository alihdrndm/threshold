import { useEffect, useState } from "react";
import { dismissWall, getSettings, quoteFor, type Quote } from "@/lib/tauri";
import { themeFor } from "@/themes";
import { chime, displaySize, exitAt, lingerFrom } from "./wall";

/**
 * The wall a blocked site puts up, answered in your own words.
 *
 * The browser's own error page cannot be changed — no policy exists for it, and
 * replacing it would need a locally trusted certificate authority, which is far
 * too high a price for decorating a failure. So the page stays the browser's and
 * this stands in front of it: a fragment of the ritual, mid-screen, for twelve
 * seconds. It wears the day's palette and the ritual's glow because it belongs
 * to the same moment - the urge - and not to the dashboard's admin world.
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

/** How long the exit fade takes; the drain line ends with it. */
const EXIT_MS = 220;

/** The settings key for the chime. Absent means on: sound is the default. */
export const WALL_SOUND_KEY = "wall_sound";

export function BlockedWindow() {
  const [quote, setQuote] = useState<Quote | null>(null);
  const [leaving, setLeaving] = useState(false);
  const linger = lingerFrom(window.location.search);

  // The day's palette, exactly as the ritual reads it. The stylesheet pins any
  // document carrying data-theme to the dark palette, so the dashboard's light
  // appearance can never reach this window - the wall is dark, always.
  useEffect(() => {
    document.documentElement.dataset.theme = themeFor(new Date()).dataAttr;
    // Lets the stylesheet clear the page background for this window alone.
    document.documentElement.dataset.window = "blocked";
  }, []);

  useEffect(() => {
    quoteFor("blocked")
      .then(setQuote)
      .catch(() => setQuote(null));
  }, []);

  // The chime, once, as the card arrives - unless it has been turned off. Read
  // from settings rather than assumed, and any failure is silence: the sound is
  // a courtesy on top of the wall, which is a courtesy on top of the block.
  useEffect(() => {
    if (!quote) return;
    getSettings()
      .then((pairs) => {
        const off = pairs.some(([k, v]) => k === WALL_SOUND_KEY && v === "off");
        if (off || typeof AudioContext === "undefined") return;
        chime(new AudioContext());
      })
      .catch(() => {});
  }, [quote]);

  // Leave the way it came. Rust destroys the window at `linger`; the fade
  // starts EXIT_MS before that so the two end together, and a click anywhere
  // sends it away early - it would have gone on its own, this only shortens
  // the wait.
  useEffect(() => {
    const timer = window.setTimeout(() => setLeaving(true), exitAt(linger, EXIT_MS));
    return () => window.clearTimeout(timer);
  }, [linger]);

  function dismiss() {
    setLeaving(true);
    window.setTimeout(() => void dismissWall(), EXIT_MS);
  }

  if (!quote) return null;

  const size = displaySize(quote.text);

  return (
    // The window is transparent; the card is the shape. p-8 leaves room for the
    // shadow and the glow to bleed past the card without being clipped by the
    // window's edge, which would draw a hard line through the light.
    <main
      className="flex h-screen w-screen items-center justify-center bg-transparent p-8"
      onClick={dismiss}
    >
      <article
        data-leaving={leaving || undefined}
        aria-live="polite"
        // The drain and the exit both read the lifetime from here, so a change
        // to LINGER in Rust changes both without touching this file.
        style={{ "--wall-linger": `${linger}ms` } as React.CSSProperties}
        className="wall-card relative flex h-full w-full flex-col justify-center overflow-hidden rounded-3xl px-12"
      >
        {/* The ritual's luminous centre, at the card's own scale. Sits behind
            the words rather than framing them: the light is the mood, the line
            is the message. Purely decorative, so reduced motion turns off its
            breath and its entrance both. */}
        <div className="wall-glow ritual-glow" aria-hidden />

        <blockquote className="ritual-step relative flex flex-col gap-4">
          <p
            data-size={size}
            className="wall-quote text-balance text-[var(--color-ink)]"
          >
            {quote.text}
          </p>
          {quote.author && (
            // The app's own eyebrow: tracked caps, muted, an em dash first -
            // the attribution belongs to the quote, and the dash says so
            // without a serif italic "cite" that would fight the display line.
            <cite className="text-[11px] tracking-[0.22em] uppercase text-[var(--color-ink-muted)] not-italic">
              — {quote.author}
            </cite>
          )}
        </blockquote>

        {/* The one honest piece of chrome: a hairline that drains over the
            card's lifetime, so nobody wonders whether they must close it. Its
            job is to say "this leaves on its own" and it says nothing else. */}
        <div className="wall-drain" aria-hidden />
      </article>
    </main>
  );
}
