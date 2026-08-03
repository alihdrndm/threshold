import { useEffect, useMemo, useState } from "react";
import { AnimatePresence } from "motion/react";
import {
  dismissPopup,
  finishRitual,
  intentionSuggestions,
  rememberCategories,
  rememberedCategories,
} from "@/lib/tauri";
import { themeById, themeFor } from "@/themes";
import {
  CATEGORIES,
  DURATIONS,
  IF_THEN_DEFAULTS,
  MAX_DURATION,
  MIN_DURATION,
  timeLabel,
} from "./copy";
import {
  Eyebrow,
  Field,
  Glow,
  Hint,
  HonourableExit,
  Pill,
  Question,
  Step,
} from "./Shell";

type StepId =
  | "arrival"
  | "intention"
  | "prediction"
  | "ifthen"
  | "commit"
  | "confirm"
  | "browsing";

export function Ritual({ trigger }: { trigger: string }) {
  const now = useMemo(() => new Date(), []);
  const theme = useMemo(() => {
    const forced = new URLSearchParams(window.location.search).get("theme");
    return themeById(forced) ?? themeFor(now);
  }, [now]);

  // "Focus on this" passes the task through, so the ritual opens already
  // knowing what you picked instead of asking again.
  const presetIntent = useMemo(
    () => new URLSearchParams(window.location.search).get("intent") ?? "",
    [],
  );

  const [step, setStep] = useState<StepId>(presetIntent ? "prediction" : "arrival");
  const [text, setText] = useState(presetIntent);
  const [blockProblem, setBlockProblem] = useState<string | null>(null);
  const [predictedYes, setPredictedYes] = useState<boolean | null>(null);
  const [ifThen, setIfThen] = useState<string>(IF_THEN_DEFAULTS[0]);
  const [customIfThen, setCustomIfThen] = useState("");
  const [duration, setDuration] = useState<number>(25);
  const [categories, setCategories] = useState<string[]>([]);
  const [suggestions, setSuggestions] = useState<string[]>([]);

  // The theme owns the palette; nothing below reads a raw colour.
  useEffect(() => {
    document.documentElement.dataset.theme = theme.dataAttr;
  }, [theme]);

  useEffect(() => {
    intentionSuggestions()
      .then(setSuggestions)
      .catch(() => setSuggestions([]));
    rememberedCategories()
      .then((saved) =>
        setCategories(saved ? saved.split(",").filter(Boolean) : []),
      )
      .catch(() => setCategories([]));
  }, []);

  function toggleCategory(id: string) {
    setCategories((current) =>
      current.includes(id) ? current.filter((c) => c !== id) : [...current, id],
    );
  }

  async function commit() {
    const joined = categories.join(",");
    await rememberCategories(joined).catch(() => {});
    setStep("confirm");

    // Arming happens here, not on a timer after the window has gone: if the
    // block cannot be applied the user has to be told, on this screen, rather
    // than being assured their sites are blocked when they are not.
    try {
      const result = await finishRitual({
        text: text.trim() || null,
        ifThen: customIfThen.trim() || ifThen,
        predictedYes,
        durationMin: duration,
        categories: joined || null,
        trigger,
        outcome: "completed",
      });
      if (result.block && !result.block.blocked) {
        setBlockProblem(result.block.reason ?? "The sites were not blocked.");
      }
    } catch (err) {
      setBlockProblem(err instanceof Error ? err.message : String(err));
    }
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

  const canContinue = theme.intentionMode === "type" ? text.trim() !== "" : true;

  return (
    <main className="relative flex h-full items-center justify-center overflow-hidden px-8">
      <Glow />

      <AnimatePresence mode="wait">
        {step === "arrival" && (
          <Step stepKey="arrival">
            <div
              aria-hidden
              className="ritual-breath h-24 w-24 rounded-full border border-[color-mix(in_srgb,var(--color-accent)_60%,transparent)]"
            />
            <Question>{theme.copy.greeting}</Question>
            <Hint>{timeLabel(now)}</Hint>
            <Pill autoFocus onClick={() => setStep("intention")}>
              Begin
            </Pill>
          </Step>
        )}

        {step === "intention" && (
          <Step stepKey="intention">
            <Eyebrow>Intention</Eyebrow>
            <Question>{theme.copy.intentionPrompt}</Question>

            {theme.intentionMode === "type" && (
              <Field
                autoFocus
                value={text}
                onChange={setText}
                onSubmit={() => text.trim() && setStep("prediction")}
                placeholder="One line is enough"
              />
            )}

            {suggestions.length > 0 && (
              <div className="flex flex-wrap justify-center gap-2">
                {suggestions.map((suggestion) => (
                  <Pill
                    key={suggestion}
                    selected={text === suggestion}
                    onClick={() => {
                      setText(suggestion);
                      setStep("prediction");
                    }}
                  >
                    {suggestion}
                  </Pill>
                ))}
              </div>
            )}

            {/* On "choose" days the field is the fallback, not the default. */}
            {theme.intentionMode === "choose" && (
              <Field
                value={text}
                onChange={setText}
                onSubmit={() => text.trim() && setStep("prediction")}
                placeholder="Or write it"
              />
            )}

            <Pill onClick={() => canContinue && setStep("prediction")}>
              Continue
            </Pill>
          </Step>
        )}

        {step === "prediction" && (
          <Step stepKey="prediction">
            <Eyebrow>Prediction</Eyebrow>
            {/* Phrased as a prediction, not an intention: the question-behaviour
                effect is strongest in this form, and strongest via a screen. */}
            <Question>{theme.copy.predictionPrompt}</Question>
            {/* Neither answer is focused. Focusing one would let a stray Enter
                from the previous step answer on the user's behalf, and would
                nudge toward that answer even when it did not — either way the
                recorded prediction stops being their real one, which is the
                only thing that makes this question worth asking. */}
            <div className="flex gap-4">
              <Pill
                onClick={() => {
                  setPredictedYes(true);
                  setStep("ifthen");
                }}
              >
                Yes
              </Pill>
              <Pill
                onClick={() => {
                  setPredictedYes(false);
                  setStep("ifthen");
                }}
              >
                No
              </Pill>
            </div>
          </Step>
        )}

        {step === "ifthen" && (
          <Step stepKey="ifthen">
            <Eyebrow>Plan</Eyebrow>
            <Question>{theme.copy.ifThenPrompt}</Question>
            <div className="flex w-full flex-col items-center gap-3">
              {IF_THEN_DEFAULTS.map((option) => (
                <Pill
                  key={option}
                  selected={!customIfThen && ifThen === option}
                  onClick={() => {
                    setIfThen(option);
                    setCustomIfThen("");
                    setStep("commit");
                  }}
                >
                  …{option}
                </Pill>
              ))}
              <Field
                value={customIfThen}
                onChange={setCustomIfThen}
                onSubmit={() => setStep("commit")}
                placeholder="…something else"
              />
            </div>
            <Pill onClick={() => setStep("commit")}>Continue</Pill>
          </Step>
        )}

        {step === "commit" && (
          <Step stepKey="commit">
            <Eyebrow>Commitment</Eyebrow>
            <Question>{theme.copy.durationPrompt}</Question>

            {theme.durationMode === "chips" ? (
              <div className="flex gap-3">
                {DURATIONS.map((minutes) => (
                  <Pill
                    key={minutes}
                    selected={duration === minutes}
                    onClick={() => setDuration(minutes)}
                  >
                    {minutes} min
                  </Pill>
                ))}
              </div>
            ) : (
              <div className="flex w-full flex-col items-center gap-3">
                <p className="text-2xl font-light text-[var(--color-ink)]">
                  {duration} min
                </p>
                <input
                  type="range"
                  min={MIN_DURATION}
                  max={MAX_DURATION}
                  step={5}
                  value={duration}
                  onChange={(event) => setDuration(Number(event.target.value))}
                  aria-label="Session length in minutes"
                  className="h-1 w-full max-w-sm cursor-pointer appearance-none rounded-full bg-[var(--color-border-subtle)] accent-[var(--color-accent)]"
                />
              </div>
            )}

            <div className="flex flex-col items-center gap-2">
              <Hint>Quiet these while you work</Hint>
              <div className="flex flex-wrap justify-center gap-2">
                {CATEGORIES.map((category) => (
                  <Pill
                    key={category.id}
                    selected={categories.includes(category.id)}
                    title={category.detail}
                    onClick={() => toggleCategory(category.id)}
                  >
                    {category.label}
                  </Pill>
                ))}
              </div>
            </div>

            <Pill onClick={() => void commit()}>Set intention</Pill>
          </Step>
        )}

        {step === "confirm" && (
          <Step stepKey="confirm">
            {/* Deliberately flat. Celebrating a stated intention licenses the
                scroll that follows it. */}
            <Question>Intention set.</Question>
            {text.trim() && <Hint>First action: {text.trim()}</Hint>}

            {/* Saying nothing here is what made blocking look like it worked
                when it had never run at all. */}
            {blockProblem && (
              <div className="mt-2 flex flex-col items-center gap-4 rounded-2xl border border-[var(--color-accent)]/50 bg-[var(--color-accent)]/[0.08] px-6 py-4">
                <p className="max-w-md text-center text-sm text-[var(--color-ink)]">
                  Your intention was saved, but the sites are <strong>not</strong>{" "}
                  blocked. {blockProblem}
                </p>
                <Pill onClick={() => void dismissPopup()}>Continue anyway</Pill>
              </div>
            )}
          </Step>
        )}

        {step === "browsing" && (
          <Step stepKey="browsing">
            <Question>Will you keep it under thirty minutes?</Question>
            {/* Same reasoning as the prediction step: an unfocused pair keeps
                the answer the user's own. */}
            <div className="flex gap-4">
              <Pill onClick={() => void browsing(true)}>Yes</Pill>
              <Pill onClick={() => void browsing(false)}>No</Pill>
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
