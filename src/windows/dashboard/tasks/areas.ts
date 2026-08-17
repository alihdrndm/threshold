import type { TaskContext } from "@/lib/tauri";

/**
 * The `#area` quick-add syntax, and nothing else about areas.
 *
 * Todoist's convention, which is the one people already have in their hands:
 * "Call the bank #personal" files the task under Personal and drops the tag
 * from the title. Kept pure so the parser can be tested without a form.
 */

/** A `#word` anywhere in the text, with what precedes and follows it. */
const TAG = /(^|\s)#([^\s#]+)/g;

export interface ParsedTitle {
  /** The title with every recognised tag removed and whitespace tidied. */
  title: string;
  /** The area the tag named, when it named one. */
  context: TaskContext | null;
  /** A tag that named no area - offered as "new area", never silently kept. */
  unknown: string | null;
}

/**
 * Pull the area out of a typed title.
 *
 * Only the first tag counts - a task has one area - and matching is by name,
 * case-insensitively, so `#Job`, `#job` and `#JOB` are one thing. A tag that
 * matches nothing is reported rather than dropped, so the UI can offer to
 * create it: a name that vanishes on Enter is a name the user thinks was saved.
 */
export function parseTitle(raw: string, contexts: TaskContext[]): ParsedTitle {
  let context: TaskContext | null = null;
  let unknown: string | null = null;

  const title = raw
    .replace(TAG, (whole, lead: string, name: string) => {
      if (context !== null || unknown !== null) return whole;
      const found = contexts.find(
        (c) => c.name.toLowerCase() === name.toLowerCase(),
      );
      if (found) {
        context = found;
        return lead;
      }
      unknown = name;
      return lead;
    })
    .replace(/\s+/g, " ")
    .trim();

  return { title, context, unknown };
}

/**
 * The tag being typed at the caret, if the caret is inside one - what the
 * autocomplete completes. `null` when the caret is not in a `#word`.
 */
export function tagAtCaret(
  text: string,
  caret: number,
): { start: number; query: string } | null {
  const before = text.slice(0, caret);
  const match = /(?:^|\s)#([^\s#]*)$/.exec(before);
  if (!match) return null;
  return { start: caret - match[1].length - 1, query: match[1] };
}

/** Areas whose names start with what was typed, in toolbar order. */
export function suggestAreas(query: string, contexts: TaskContext[]): TaskContext[] {
  const q = query.toLowerCase();
  return contexts.filter((c) => c.name.toLowerCase().startsWith(q));
}
