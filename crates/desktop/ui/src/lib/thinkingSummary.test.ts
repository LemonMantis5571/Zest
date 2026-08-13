import assert from "node:assert/strict";
import test from "node:test";

import {
  countThinkingSteps,
  lastThinkingLine,
  thinkingTraceRows,
  thinkingSummaryLabel,
} from "./thinkingSummary.ts";

/**
 * The shipped symptom: twenty summarized steps rendered as a column of headings
 * taller than the viewport, pushing the actual answer off screen.
 */
const STREAM = [
  "**Planning delegation with context gathering**",
  "",
  "Looking at what the worker needs to know.",
  "",
  "**Drafting self-contained delegation task**",
  "",
  "Writing it so it stands alone.",
  "",
  "**Analyzing migration and test update requirements**",
  "",
  "Checking which tests move.",
].join("\n");

test("the newest step title is what the single line shows", () => {
  assert.equal(
    lastThinkingLine(STREAM),
    "Analyzing migration and test update requirements"
  );
});

test("a half-streamed title still resolves to the previous complete one", () => {
  assert.equal(lastThinkingLine(`${STREAM}\n\n**Refining hir`), "Analyzing migration and test update requirements");
});

test("untitled prose falls back to its own tail rather than going blank", () => {
  const prose = "Let me look at the config.\nThe provider list comes from Rust.";
  assert.equal(lastThinkingLine(prose), "The provider list comes from Rust.");
});

test("stray emphasis never reaches the visible line as literal asterisks", () => {
  assert.equal(lastThinkingLine("Checking the **gateway** now"), "Checking the gateway now");
});

test("empty thinking yields an empty line, not a crash", () => {
  assert.equal(lastThinkingLine(""), "");
  assert.equal(lastThinkingLine("\n\n  \n"), "");
});

test("step counting drives the settled summary label", () => {
  assert.equal(countThinkingSteps(STREAM), 3);
  assert.equal(thinkingSummaryLabel(STREAM), "Thought through 3 steps");
  assert.equal(thinkingSummaryLabel("**Only one**"), "Thought through 1 step");
  assert.equal(thinkingSummaryLabel("plain prose"), "Thought about this");
});

test("thinking rows pair each title with the prose that explains it", () => {
  assert.deepEqual(thinkingTraceRows(STREAM), [
    {
      primary: "Planning delegation with context gathering",
      secondary: "Looking at what the worker needs to know.",
      kind: "step",
    },
    {
      primary: "Drafting self-contained delegation task",
      secondary: "Writing it so it stands alone.",
      kind: "step",
    },
    {
      primary: "Analyzing migration and test update requirements",
      secondary: "Checking which tests move.",
      kind: "step",
    },
  ]);
});

test("untitled thinking still produces a useful detail row", () => {
  assert.deepEqual(thinkingTraceRows("Checking the **gateway** now."), [
    { primary: "Checking the gateway now.", kind: "detail" },
  ]);
});
