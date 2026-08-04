/**
 * Copy for the closing question.
 *
 * The framing here is load-bearing rather than stylistic. A lapse presented as
 * a failure is what produces abandonment — one bad session becomes "this isn't
 * working" becomes uninstalled. So there is no verdict on this screen, and no
 * encouragement either: reassurance after "not this time" tells you there was
 * something to be reassured about, and praise after "did it" is a reward at the
 * moment the spec already warns against rewarding.
 *
 * Nothing happens after you answer. A screen that thanks you for reporting is a
 * screen you learn to close before reading.
 */
export const CHECKIN = {
  headline: (minutes: number, subject: string | null) =>
    subject
      ? `${minutes} ${minutes === 1 ? "minute" : "minutes"} on ${subject}.`
      : `${minutes} ${minutes === 1 ? "minute" : "minutes"}.`,

  // Said flatly, in the past tense, with no "but". It is the prediction being
  // scored, not the person.
  predictedYes: "You thought you would.",
  predictedNo: "You thought you wouldn't.",
  noPrediction: "No prediction on this one.",

  answers: {
    // Three, not two. A binary forces a partly-true session into one of two
    // lies, and the lie people pick is the harsh one.
    did_it: "Did it",
    partly: "Partly",
    // "Not this time" rather than a bare "No": the same information, with the
    // time-bounding built in, which is precisely the framing that stops one
    // lapse generalising into a verdict.
    no: "Not this time",
  },

  markDone: "Mark this task done",
  dismiss: "Not now",

  /** Said only when true, and said plainly rather than apologetically. */
  blockHeld: (minutes: number) =>
    `Your sites stay quiet for about ${minutes} more ${minutes === 1 ? "minute" : "minutes"}.`,
} as const;
