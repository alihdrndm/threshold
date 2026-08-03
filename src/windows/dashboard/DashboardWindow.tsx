import { useEffect, useState } from "react";
import { motion } from "motion/react";
import { appVersion, isTauri, ping } from "@/lib/tauri";

/**
 * Phase 0 shell. Its job is to prove the pipeline end to end — WebView2 boots,
 * the JS API bridge resolves, an invoke() round-trips to Rust, Tailwind tokens
 * apply, and Motion animates — not to look like the finished dashboard (F4).
 */
export function DashboardWindow() {
  const [version, setVersion] = useState("…");
  const [pong, setPong] = useState<string | null>(null);

  useEffect(() => {
    if (!isTauri()) {
      setVersion("browser");
      return;
    }
    appVersion().then(setVersion).catch(() => setVersion("unknown"));
  }, []);

  return (
    <main className="flex h-full items-center justify-center p-12">
      <motion.div
        initial={{ opacity: 0, y: 8 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.3, ease: [0.16, 1, 0.3, 1] }}
        className="w-full max-w-md rounded-2xl border border-[var(--color-border-subtle)] bg-[var(--color-surface-raised)] p-10"
      >
        <h1 className="text-2xl font-medium tracking-tight">Threshold</h1>
        <p className="mt-2 text-sm text-[var(--color-ink-muted)]">
          Phase 0 scaffold — version {version}
        </p>

        <button
          type="button"
          onClick={() => ping().then(setPong).catch((e) => setPong(String(e)))}
          className="mt-8 rounded-lg border border-[var(--color-border-subtle)] px-4 py-2 text-sm text-[var(--color-ink)] transition-colors duration-150 hover:bg-white/5"
        >
          Ping the core
        </button>

        {pong && (
          <p className="mt-4 text-sm text-[var(--color-accent)]">{pong}</p>
        )}
      </motion.div>
    </main>
  );
}
