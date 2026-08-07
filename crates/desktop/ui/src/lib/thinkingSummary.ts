/**
 * Reducing a growing thinking stream to the one line worth showing.
 *
 * Summarized thinking arrives as a run of `**Title**` blocks, each with a
 * paragraph under it. Rendered in full while a turn runs, twenty of those stack
 * into a column taller than the viewport that pushes the actual answer off
 * screen — so the transcript shows the newest line and keeps the rest behind a
 * disclosure.
 */

/** A `**Bold title**` occupying a whole line — how each step announces itself. */
const TITLE_LINE = /^\s*\*\*(.+?)\*\*\s*$/;

function lines(thinking: string): string[] {
  return thinking.split("\n").map((line) => line.trim());
}

/**
 * The newest step title, or the newest non-empty line when there are none.
 *
 * Falls back rather than returning empty: unsummarized providers stream plain
 * prose with no titles at all, and those turns still need something to show.
 */
export function lastThinkingLine(thinking: string): string {
  const all = lines(thinking).filter(Boolean);
  if (all.length === 0) return "";

  for (let i = all.length - 1; i >= 0; i -= 1) {
    const match = TITLE_LINE.exec(all[i]);
    if (match) return match[1].trim();
  }

  // No titles: the tail of the prose is the best available "where we are now".
  // Stripped of emphasis so a half-streamed `**` does not render as literal
  // asterisks on the one line the user actually sees.
  return all[all.length - 1].replace(/\*\*/g, "").trim();
}

/** How many titled steps the stream has produced. Zero for untitled prose. */
export function countThinkingSteps(thinking: string): number {
  return lines(thinking).filter((line) => TITLE_LINE.test(line)).length;
}

/** Label for the collapsed disclosure once a turn has settled. */
export function thinkingSummaryLabel(thinking: string): string {
  const steps = countThinkingSteps(thinking);
  if (steps === 0) return "Thought about this";
  return `Thought through ${steps} step${steps === 1 ? "" : "s"}`;
}
