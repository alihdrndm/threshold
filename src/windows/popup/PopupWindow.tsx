/**
 * Fullscreen intention ritual. Built in Phase 2 (spec F1); this stub exists so
 * the multi-window routing in windows/registry.tsx is exercised from Phase 0
 * rather than retrofitted.
 */
export function PopupWindow() {
  return (
    <main className="flex h-full items-center justify-center">
      <p className="text-[var(--color-ink-muted)]">Popup window — Phase 2.</p>
    </main>
  );
}
