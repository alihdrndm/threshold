import { useEffect, useMemo, useRef, useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import {
  finishRitual,
  intentionSuggestions,
  rememberCategories,
  rememberedCategories,
} from "@/lib/tauri";
import {
  CATEGORIES,
  DURATIONS,
  IF_THEN_DEFAULTS,
  greetingFor,
  timeLabel,
} from "./copy";
import { Choice, Hint, HonourableExit, Question, Step } from "./Shell";

type StepId =
  | "arrival"
  | "intention"
  | "prediction"
  | "ifthen"
  | "commit"
  | "confirm"
  | "browsing";

export function Ritual({ trigger }: { trigger: string }) {
  const [step, setStep] = useState<StepId>("arrival");
  const [text, setText] = useState("");
  const [predictedYes, setPredictedYes] = useState<boolean | null>(null);
  const [ifThen, setIfThen] = useState<string>(IF_THEN_DEFAULTS[0]);
  const [duration, setDuration] = useState<number>(25);
  const [categories, setCategories] = useState<string[]>([]);
  const [suggestions, setSuggestions] = useState<string[]>([]);

  const now = useMemo(() => new Date(), []);

  useEffect(() => {
    intentionSuggestions().then(setSuggestions).catch(() => setSuggestions([]));
    rememberedCategories()
      .then((saved) => setCategories(saved ? saved.split(",").filter(Boolean) : []))
      .catch(() => setCategories([]));
  }, []);

  function toggleCategory(id: string) {
    setCategories((current) =>
      current.includes(id)
        ? current.filter((c) => c !== id)
        : [...current, id],
    );
  }

  async function commit() {
    const joined = categories.join(",");
    await rememberCategories(joined).catch(() => {});
    setStep("confirm");
    // The confirmation is read, not clicked past. Deliberately plain.
    setTimeout(() => {
      void finishRitual({
        text: text.trim() || null,
        ifThen,
        predictedYes,
        durationMin: duration,
        categories: joined || null,
        trigger,
        outcome: "completed",
      });
    }, 2200);
  }

  /** Honourable exit: one binary question, no blocks, no guilt, then gone. */
  async function browsing(underThirty: boolean) {
    await finishRitual({
      text: null,
      ifThen: null,
      predictedYes: underThirty,
      durationMin: null,
      categories: null,
      trigger,
      outcome: "browsing",
    });
  }

  return (
    <main className="relative flex h-full flex-col items-center justify-center px-8">
      <AnimatePresence mode="wait">
        {step === "arrival" && (
          <Step stepKey="arrival">
            <Breath onDone={() => setStep("intention")} />
            <Question>{greetingFor(now)}</Question>
            <Hint>{timeLabel(now)}</Hint>
            <Choice onClick={() => setStep("intention")} autoFocus>
              Begin
            </Choice>
          </Step>
        )}

        {step === "intention" && (
          <Step stepKey="intention">
            <Question>What are you here for?</Question>
            <input
              autoFocus
              value={text}
              onChange={(e) => setText(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && text.trim()) setStep("prediction");
              }}
              placeholder="One line is enough"
              className="w-full border-b border-[var(--color-border-subtle)] bg-transparent pb-3 text-center text-xl text-[var(--color-ink)] outline-none transition-colors duration-150 placeholder:text-[var(--color-ink-muted)]/60 focus:border-[var(--color-accent)]"
            />
            {suggestions.length > 0 && (
              <div className="flex flex-wrap justify-center gap-2">
                {suggestions.map((suggestion) => (
                  <button
                    key={suggestion}
                    type="button"
                    onClick={() => {
                      setText(suggestion);
                      setStep("prediction");
                    }}
                    className="rounded-full border border-[var(--color-border-subtle)] px-4 py-1.5 text-sm text-[var(--color-ink-muted)] transition-colors duration-150 hover:bg-white/5 hover:text-[var(--color-ink)]"
                  >
                    {suggestion}
                  </button>
                ))}
              </div>
            )}
            <Choice onClick={() => text.trim() && setStep("prediction")}>
              Continue
            </Choice>
          </Step>
        )}

        {step === "prediction" && (
          <Step stepKey="prediction">
            {/* Phrased as a prediction, not an intention: the question-behaviour
                effect is strongest in this form, and strongest via a screen. */}
            <Question>Will you start this before opening anything else?</Question>
            {/* Neither answer is focused. Focusing one would let a stray Enter
                from the previous step answer on the user's behalf, and would
                nudge toward that answer even when it does not — either way the
                recorded prediction stops being their real one, which is the
                only thing that makes this question worth asking. */}
            <div className="flex gap-4">
              <Choice
                onClick={() => {
                  setPredictedYes(true);
                  setStep("ifthen");
                }}
              >
                Yes
              </Choice>
              <Choice
                onClick={() => {
                  setPredictedYes(false);
                  setStep("ifthen");
                }}
              >
                No
              </Choice>
            </div>
          </Step>
        )}

        {step === "ifthen" && (
          <Step stepKey="ifthen">
            <Question>If I feel the urge to open a feed, then I will…</Question>
            <div className="flex w-full flex-col gap-3">
              {IF_THEN_DEFAULTS.map((option) => (
                <button
                  key={option}
                  type="button"
                  onClick={() => {
                    setIfThen(option);
                    setStep("commit");
                  }}
                  className={`rounded-xl border px-5 py-3 text-left text-base transition-colors duration-150 ${
                    ifThen === option
                      ? "border-[var(--color-accent)] text-[var(--color-ink)]"
                      : "border-[var(--color-border-subtle)] text-[var(--color-ink-muted)] hover:bg-white/5 hover:text-[var(--color-ink)]"
                  }`}
                >
                  …{option}
                </button>
              ))}
              <input
                value={IF_THEN_DEFAULTS.includes(ifThen as never) ? "" : ifThen}
                onChange={(e) => setIfThen(e.target.value)}
                placeholder="…something else"
                className="rounded-xl border border-[var(--color-border-subtle)] bg-transparent px-5 py-3 text-base text-[var(--color-ink)] outline-none transition-colors duration-150 placeholder:text-[var(--color-ink-muted)]/60 focus:border-[var(--color-accent)]"
              />
            </div>
            <Choice onClick={() => setStep("commit")}>Continue</Choice>
          </Step>
        )}

        {step === "commit" && (
          <Step stepKey="commit">
            <Question>How long?</Question>
            <div className="flex gap-3">
              {DURATIONS.map((minutes) => (
                <Choice
                  key={minutes}
                  selected={duration === minutes}
                  autoFocus={minutes === DURATIONS[0]}
                  onClick={() => setDuration(minutes)}
                >
                  {minutes} min
                </Choice>
              ))}
            </div>

            <div className="flex w-full flex-col gap-2">
              <Hint>Quiet these while you work</Hint>
              <div className="flex flex-wrap justify-center gap-2">
                {CATEGORIES.map((category) => (
                  <button
                    key={category.id}
                    type="button"
                    onClick={() => toggleCategory(category.id)}
                    className={`rounded-full border px-4 py-2 text-sm transition-colors duration-150 ${
                      categories.includes(category.id)
                        ? "border-[var(--color-accent)] bg-[var(--color-accent)]/10 text-[var(--color-ink)]"
                        : "border-[var(--color-border-subtle)] text-[var(--color-ink-muted)] hover:bg-white/5"
                    }`}
                    title={category.detail}
                  >
                    {category.label}
                  </button>
                ))}
              </div>
            </div>

            <Choice onClick={() => void commit()}>Set intention</Choice>
          </Step>
        )}

        {step === "confirm" && (
          <Step stepKey="confirm">
            {/* Deliberately flat. Celebrating a stated intention licenses the
                scroll that follows it. */}
            <Question>Intention set.</Question>
            {text.trim() && <Hint>First action: {text.trim()}</Hint>}
          </Step>
        )}

        {step === "browsing" && (
          <Step stepKey="browsing">
            <Question>Will you keep it under thirty minutes?</Question>
            {/* Same reasoning as the prediction step: an unfocused pair keeps
                the answer the user's own. */}
            <div className="flex gap-4">
              <Choice onClick={() => void browsing(true)}>
                Yes
              </Choice>
              <Choice onClick={() => void browsing(false)}>No</Choice>
            </div>
          </Step>
        )}
      </AnimatePresence>

      {step !== "confirm" && step !== "browsing" && (
        <HonourableExit onClick={() => setStep("browsing")} />
      )}
    </main>
  );
}

/**
 * A three second expanding circle. Skippable by clicking, because a breath you
 * are forced to take is just a delay.
 */
function Breath({ onDone }: { onDone: () => void }) {
  const done = useRef(false);
  useEffect(() => {
    const timer = setTimeout(() => {
      done.current = true;
    }, 3000);
    return () => clearTimeout(timer);
  }, [onDone]);

  return (
    <motion.div
      aria-hidden
      initial={{ scale: 0.6, opacity: 0.25 }}
      animate={{ scale: 1, opacity: 0.5 }}
      transition={{ duration: 3, ease: "easeInOut" }}
      className="h-24 w-24 rounded-full border border-[var(--color-accent)]"
    />
  );
}
