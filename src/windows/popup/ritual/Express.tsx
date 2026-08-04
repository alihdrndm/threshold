import { useState } from "react";
import type { Theme } from "@/themes/types";
import { CATEGORIES, DURATIONS, EXPRESS, IF_THEN_DEFAULTS } from "./copy";
import { Eyebrow, Field, Hint, Pill, Question, Step } from "./Shell";

/**
 * One screen, because the decision is already made.
 *
 * The full ritual is for arriving at the machine with nothing chosen. This is
 * for arriving having chosen: you picked the task, so the only open questions
 * are how long, what to quiet, and what you will do when the urge arrives.
 *
 * The prediction is *asked*, not pre-filled, and Start stays refused until it
 * is answered. That costs one tap and it is worth it: the closing check-in
 * scores itself against this answer, and a default would mean scoring against
 * something nobody said. The full ritual deliberately focuses neither button
 * for the same reason.
 */
export function ExpressStep({
  theme,
  title,
  ifThen,
  onIfThen,
  customIfThen,
  onCustomIfThen,
  predictedYes,
  onPrediction,
  duration,
  onDuration,
  categories,
  onToggleCategory,
  onStart,
}: {
  theme: Theme;
  title: string;
  ifThen: string;
  onIfThen: (value: string) => void;
  customIfThen: string;
  onCustomIfThen: (value: string) => void;
  predictedYes: boolean | null;
  onPrediction: (value: boolean) => void;
  duration: number;
  onDuration: (value: number) => void;
  categories: string[];
  onToggleCategory: (id: string) => void;
  onStart: () => void;
}) {
  const [writing, setWriting] = useState(false);

  return (
    <Step stepKey="express">
      <Eyebrow>{EXPRESS.eyebrow}</Eyebrow>

      {/* The task, verbatim. Not rephrased into a prompt: the whole reason this
          path exists is that the question was already answered. */}
      <Question>{title}</Question>

      {/* One wrapping row rather than the full ritual's stacked column: this
          screen is fullscreen but has five sections to fit on a 768px display. */}
      <div className="flex flex-col items-center gap-2">
        <Hint>{theme.copy.ifThenPrompt}</Hint>
        <div className="flex flex-wrap justify-center gap-2">
          {IF_THEN_DEFAULTS.map((option) => (
            <Pill
              key={option}
              size="sm"
              selected={!writing && !customIfThen && ifThen === option}
              onClick={() => {
                setWriting(false);
                onCustomIfThen("");
                onIfThen(option);
              }}
            >
              …{option}
            </Pill>
          ))}
          {!writing && (
            <Pill size="sm" onClick={() => setWriting(true)}>
              …something else
            </Pill>
          )}
        </div>
        {writing && (
          <Field
            autoFocus
            value={customIfThen}
            onChange={onCustomIfThen}
            placeholder="…something else"
          />
        )}
      </div>

      <div className="flex flex-col items-center gap-2">
        <Hint>{theme.copy.predictionPrompt}</Hint>
        <div className="flex gap-2">
          <Pill
            size="sm"
            selected={predictedYes === true}
            onClick={() => onPrediction(true)}
          >
            {EXPRESS.predictYes}
          </Pill>
          <Pill
            size="sm"
            selected={predictedYes === false}
            onClick={() => onPrediction(false)}
          >
            {EXPRESS.predictNo}
          </Pill>
        </div>
      </div>

      {/* Chips every day, even on the rotation's slider days. The rotation
          exists so the ritual cannot habituate; this screen is entered by a
          deliberate click and has one screen of room, so it takes the compact
          control. */}
      <div className="flex flex-col items-center gap-2">
        <Hint>{theme.copy.durationPrompt}</Hint>
        <div className="flex gap-2">
          {DURATIONS.map((minutes) => (
            <Pill
              key={minutes}
              size="sm"
              selected={duration === minutes}
              onClick={() => onDuration(minutes)}
            >
              {minutes} min
            </Pill>
          ))}
        </div>
      </div>

      <div className="flex flex-col items-center gap-2">
        <Hint>{EXPRESS.categories}</Hint>
        <div className="flex flex-wrap justify-center gap-2">
          {CATEGORIES.map((category) => (
            <Pill
              key={category.id}
              size="sm"
              title={category.detail}
              selected={categories.includes(category.id)}
              onClick={() => onToggleCategory(category.id)}
            >
              {category.label}
            </Pill>
          ))}
        </div>
      </div>

      <Pill onClick={onStart} disabled={predictedYes === null}>
        {EXPRESS.start(duration)}
      </Pill>
    </Step>
  );
}
