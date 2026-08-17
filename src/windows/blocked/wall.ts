/**
 * The pure parts of the wall: how big the line is set, and what it sounds like.
 * Kept apart from the window so both can be tested without a webview.
 */

/**
 * A display size that fits the line. Long quotes step down rather than wrap
 * into a paragraph - a paragraph is read; a line at this size is heard.
 * Thresholds are character counts at the card's measure (~560px), chosen so
 * each step lands within three lines.
 */
export function displaySize(text: string): "lg" | "md" | "sm" {
  const length = text.trim().length;
  if (length <= 90) return "lg";
  if (length <= 170) return "md";
  return "sm";
}

/**
 * When the exit fade should begin so it ends as the window is destroyed.
 * `linger` is what Rust said; a page that guessed would fade too early or be
 * cut off mid-fade, and both read as a fault.
 */
export function exitAt(lingerMs: number, fadeMs: number): number {
  return Math.max(0, lingerMs - fadeMs);
}

/** The window's own reading of its lifetime, from the URL Rust opened it at. */
export function lingerFrom(search: string, fallbackMs = 12_000): number {
  const raw = Number(new URLSearchParams(search).get("linger"));
  return Number.isFinite(raw) && raw > 0 ? raw : fallbackMs;
}

/**
 * Two soft notes, a fifth apart, synthesised on the spot.
 *
 * No asset to ship or to fail loading, and quiet by design: this is a tap on
 * the shoulder, not an alarm. Sine tones with a fast rise and a long fall are
 * the shape of a struck bell with the metal taken out. Any failure - a
 * webview without audio, a policy that holds it - is swallowed: the sound is
 * a courtesy on top of the wall, which is a courtesy on top of the block.
 */
export function chime(context: AudioContextLike): void {
  const now = context.currentTime;
  const notes: [frequency: number, at: number][] = [
    [523.25, 0], // C5
    [783.99, 0.14], // G5
  ];
  for (const [frequency, at] of notes) {
    const osc = context.createOscillator();
    const gain = context.createGain();
    osc.type = "sine";
    osc.frequency.value = frequency;
    gain.gain.setValueAtTime(0.0001, now + at);
    gain.gain.exponentialRampToValueAtTime(0.09, now + at + 0.012);
    gain.gain.exponentialRampToValueAtTime(0.0001, now + at + 1.4);
    osc.connect(gain);
    gain.connect(context.destination);
    osc.start(now + at);
    osc.stop(now + at + 1.5);
  }
}

/** Just enough of AudioContext to synthesise the chime - and to fake in tests. */
export interface AudioContextLike {
  currentTime: number;
  destination: AudioNode;
  createOscillator(): OscillatorNode;
  createGain(): GainNode;
}
