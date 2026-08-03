import type { ReactNode } from "react";
import { motion } from "motion/react";

/**
 * One question visible at a time, generous space around it.
 *
 * Steps cross-fade with a small upward drift: enter eases out over 320ms, well
 * inside the spec's 400ms ceiling, so the sequence never feels like it is
 * making you wait.
 */
export function Step({
  children,
  stepKey,
}: {
  children: ReactNode;
  stepKey: string;
}) {
  return (
    <motion.div
      key={stepKey}
      initial={{ opacity: 0, y: 14 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: -10 }}
      transition={{ duration: 0.32, ease: [0.16, 1, 0.3, 1] }}
      className="flex w-full max-w-xl flex-col items-center gap-8"
    >
      {children}
    </motion.div>
  );
}

export function Question({ children }: { children: ReactNode }) {
  return (
    <h1 className="text-center text-3xl font-light tracking-tight text-[var(--color-ink)]">
      {children}
    </h1>
  );
}

export function Hint({ children }: { children: ReactNode }) {
  return (
    <p className="text-center text-sm text-[var(--color-ink-muted)]">
      {children}
    </p>
  );
}

export function Choice({
  children,
  onClick,
  selected = false,
  autoFocus = false,
}: {
  children: ReactNode;
  onClick: () => void;
  selected?: boolean;
  autoFocus?: boolean;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      autoFocus={autoFocus}
      className={`rounded-xl border px-6 py-3 text-base transition-colors duration-150 ${
        selected
          ? "border-[var(--color-accent)] bg-[var(--color-accent)]/10 text-[var(--color-ink)]"
          : "border-[var(--color-border-subtle)] text-[var(--color-ink-muted)] hover:bg-white/5 hover:text-[var(--color-ink)]"
      }`}
    >
      {children}
    </button>
  );
}

/**
 * The honourable exit. Always visible, always one click, never worded to make
 * taking it feel like a failure — the RCT this design follows found the
 * explicit dismiss option to be the single most effective feature, and hiding
 * or shaming it is what gets tools like this uninstalled.
 */
export function HonourableExit({ onClick }: { onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="absolute bottom-10 text-sm text-[var(--color-ink-muted)] underline-offset-4 transition-colors duration-150 hover:text-[var(--color-ink)] hover:underline"
    >
      Just browsing today
    </button>
  );
}
