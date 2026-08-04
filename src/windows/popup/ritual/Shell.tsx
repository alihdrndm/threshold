import type { ReactNode } from "react";
import { motion, useReducedMotion } from "motion/react";

/**
 * Focal: content sits inside a luminous centre, controls are pills.
 *
 * Motion notes, all deliberate:
 *  - Entrances use a strong ease-out at 260ms. Never ease-in: it delays the
 *    moment the eye is watching most closely.
 *  - Exits are faster (160ms). Slow where the user is deciding, fast where the
 *    system is responding.
 *  - Transforms are written as full strings rather than Motion's x/y/scale
 *    shorthands, which run on the main thread and drop frames under load.
 */

const EASE_OUT = [0.23, 1, 0.32, 1] as const;

export function Glow() {
  return <div className="ritual-glow" aria-hidden />;
}

export function Step({
  children,
  stepKey,
}: {
  children: ReactNode;
  stepKey: string;
}) {
  const reduce = useReducedMotion();

  // The parent only fades. Movement and stagger live in CSS on the children
  // (.ritual-step), so the two layers never animate the same property.
  return (
    <motion.div
      key={stepKey}
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{
        opacity: 0,
        transform: reduce ? "none" : "translateY(-6px)",
        // Exit snaps: the user has already decided, and waiting on the old
        // screen to leave is the part that feels slow.
        transition: { duration: 0.16, ease: EASE_OUT },
      }}
      transition={{ duration: 0.22, ease: EASE_OUT }}
      className="ritual-step relative flex w-full max-w-xl flex-col items-center gap-7"
    >
      {children}
    </motion.div>
  );
}

export function Eyebrow({ children }: { children: ReactNode }) {
  return (
    <p className="text-xs tracking-[0.24em] text-[var(--color-ink)]/50 uppercase">
      {children}
    </p>
  );
}

export function Question({ children }: { children: ReactNode }) {
  return (
    <h1 className="text-center text-3xl font-light tracking-tight text-balance text-[var(--color-ink)]">
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

/** Shared pill. `scale(0.97)` on press so the interface feels like it heard you. */
export function Pill({
  children,
  onClick,
  selected = false,
  autoFocus = false,
  title,
}: {
  children: ReactNode;
  onClick: () => void;
  selected?: boolean;
  autoFocus?: boolean;
  title?: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      autoFocus={autoFocus}
      title={title}
      data-selected={selected || undefined}
      className={`ritual-pressable rounded-full border px-7 py-3 text-base focus-visible:outline-2 focus-visible:outline-offset-[3px] focus-visible:outline-[var(--color-accent)] ${
        selected
          ? "border-[var(--color-accent)] bg-[color-mix(in_srgb,var(--color-accent)_12%,transparent)] text-[var(--color-ink)]"
          : "border-[var(--color-border-subtle)] bg-[var(--color-fill-subtle)] text-[var(--color-ink)]"
      }`}
    >
      {children}
    </button>
  );
}

export function Field({
  value,
  onChange,
  onSubmit,
  placeholder,
  autoFocus = false,
}: {
  value: string;
  onChange: (value: string) => void;
  onSubmit?: () => void;
  placeholder: string;
  autoFocus?: boolean;
}) {
  return (
    <input
      autoFocus={autoFocus}
      value={value}
      onChange={(event) => onChange(event.target.value)}
      onKeyDown={(event) => {
        if (event.key === "Enter" && onSubmit) onSubmit();
      }}
      placeholder={placeholder}
      className="ritual-field w-full rounded-full border border-[var(--color-border-subtle)] bg-[var(--color-fill-subtle)] px-7 py-4 text-center text-lg text-[var(--color-ink)] outline-none placeholder:text-[var(--color-ink-muted)] focus:border-[color-mix(in_srgb,var(--color-accent)_65%,transparent)] focus:bg-[color-mix(in_srgb,var(--color-accent)_6%,transparent)]"
    />
  );
}

/**
 * The honourable exit. Always present, always one click, never worded so that
 * taking it reads as failure — the RCT this design follows found the explicit
 * dismiss option to be its single most effective feature, and hiding or shaming
 * it is what gets tools like this uninstalled.
 */
export function HonourableExit({ onClick }: { onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="ritual-exit absolute bottom-10 z-10 rounded-full px-4 py-2 text-sm text-[var(--color-ink-muted)] underline-offset-4 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--color-accent)]"
    >
      Just browsing today
    </button>
  );
}
