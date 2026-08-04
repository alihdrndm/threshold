import type { ReactNode } from "react";

/**
 * The one strip under the tab bar.
 *
 * Its job is to explain a state that changes what the rest of the window
 * means, so there is only ever one of them on screen. Two stacked would push
 * the content down twice and read as an error rather than as two facts.
 *
 * Extracted from the pause banner rather than written alongside it, so the two
 * cannot drift apart.
 */
export function Banner({
  children,
  action,
}: {
  children: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div className="banner-in flex items-center gap-4 border-b border-[var(--color-accent)]/40 bg-[var(--color-accent)]/[0.08] px-8 py-3">
      <p className="flex-1 text-sm">{children}</p>
      {action}
    </div>
  );
}

export function BannerAction({
  children,
  onClick,
  label,
}: {
  children: ReactNode;
  onClick: () => void;
  label?: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-label={label}
      className="ritual-pressable shrink-0 rounded-full border border-[var(--color-border-subtle)] px-4 py-1.5 text-sm"
    >
      {children}
    </button>
  );
}
