import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
  effortFromSession,
  mergeSessionOptions,
  rollbackSessionOptions,
} from "./sessionOptions.ts";
import type { SessionInfo } from "./types.ts";

const base: SessionInfo = {
  sessionId: "s1",
  provider: "codex",
  label: "Codex",
  model: "gpt-5.4",
  effort: "high",
  root: ".",
  threadId: "t1",
  defaultModel: "gpt-5.6-sol",
  models: [
    { id: "gpt-5.6-sol", efforts: ["low", "medium", "high", "xhigh", "max"] },
    { id: "gpt-5.4", efforts: ["low", "medium", "high", "xhigh", "max"] },
  ],
  messages: [{ id: "u1", role: "user", text: "hi" }],
};

describe("session options authority", () => {
  it("merges Rust session info but keeps local messages", () => {
    const info: SessionInfo = {
      ...base,
      model: "gpt-5.3",
      effort: "medium",
      messages: [],
    };
    const merged = mergeSessionOptions(base, info);
    assert.equal(merged.model, "gpt-5.3");
    assert.equal(merged.effort, "medium");
    assert.equal(merged.messages.length, 1);
  });

  it("rolls back optimistic model/effort on failure", () => {
    const optimistic: SessionInfo = {
      ...base,
      model: "bad-model",
      effort: "low",
    };
    const rolled = rollbackSessionOptions(optimistic, {
      model: base.model,
      effort: "high",
    });
    assert.ok(rolled);
    assert.equal(rolled?.model, "gpt-5.4");
    assert.equal(rolled?.effort, "high");
  });

  it("maps unknown effort to fallback", () => {
    assert.equal(effortFromSession("nope", "high"), "high");
    assert.equal(effortFromSession("xhigh", "high"), "xhigh");
  });
});
