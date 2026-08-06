import { useState } from "react";
import { checkSite } from "@/lib/tauri";

/**
 * Adding and removing sites, shared by the ritual and by Settings.
 *
 * The two screens look nothing alike — one is a dark fullscreen takeover, the
 * other a panel — so they render their own chrome. What they must not have is
 * their own idea of what a valid site is: two copies of this would drift, and
 * the half that drifted would be the one that quietly accepted something the
 * elevated helper then refused.
 *
 * Resolution happens in Rust before anything becomes a chip, so a chip always
 * shows what will actually be blocked rather than an echo of the typing.
 */
export function useSiteEditor(
  sites: string[],
  onChange: (next: string[]) => void,
) {
  const [typed, setTyped] = useState("");
  const [problem, setProblem] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);

  async function add() {
    const candidate = typed.trim();
    if (!candidate || checking) return;

    setChecking(true);
    try {
      const host = await checkSite(candidate);
      // Already on the list is not an error — the chip below is the answer.
      if (!sites.includes(host)) onChange([...sites, host]);
      setTyped("");
      setProblem(null);
    } catch (reason) {
      setProblem(String(reason));
    } finally {
      setChecking(false);
    }
  }

  return {
    typed,
    problem,
    checking,
    remove: (site: string) => onChange(sites.filter((s) => s !== site)),
    add,
    onType(value: string) {
      setTyped(value);
      // Clearing on the next keystroke: a refusal that outlives the thing it
      // refused reads as the field being broken.
      if (problem) setProblem(null);
    },
  };
}
