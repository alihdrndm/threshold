import { motion } from "motion/react";
import { dismissPopup } from "@/lib/tauri";

/**
 * Phase 1 placeholder for the intention ritual (spec F1, built in Phase 2).
 *
 * It shows which trigger opened it, so the manual test matrix — cold boot,
 * wake, unlock — can be verified by looking at the screen. The exit is already
 * here and already one click, because that is the single most effective part
 * of the whole design and everything else gets built around it.
 */
export function PopupWindow() {
  const trigger =
    new URLSearchParams(window.location.search).get("trigger") ?? "unknown";

  return (
    <main className="flex h-full flex-col items-center justify-center gap-10 bg-[var(--color-surface)]">
      <motion.div
        initial={{ opacity: 0, y: 12 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.4, ease: [0.16, 1, 0.3, 1] }}
        className="flex flex-col items-center gap-3"
      >
        <p className="text-sm tracking-widest text-[var(--color-ink-muted)] uppercase">
          {trigger}
        </p>
        <h1 className="text-4xl font-light tracking-tight">
          The ritual goes here.
        </h1>
        <p className="max-w-md text-center text-[var(--color-ink-muted)]">
          Phase 1 proves the moment was caught. Phase 2 fills in the questions.
        </p>
      </motion.div>

      <motion.button
        type="button"
        onClick={() => void dismissPopup()}
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ duration: 0.3, delay: 0.15 }}
        className="rounded-lg border border-[var(--color-border-subtle)] px-5 py-2.5 text-sm text-[var(--color-ink-muted)] transition-colors duration-150 hover:bg-white/5 hover:text-[var(--color-ink)]"
      >
        Just browsing today
      </motion.button>
    </main>
  );
}
