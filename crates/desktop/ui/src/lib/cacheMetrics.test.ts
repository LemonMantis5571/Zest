import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { cacheMetrics } from "./cacheMetrics.ts";

describe("cache metrics", () => {
  it("uses the full prompt volume for the hit rate", () => {
    const metrics = cacheMetrics([
      { measured: { inputTokens: 1_000, cacheReadTokens: 9_000, cacheWriteTokens: 500 } },
    ]);

    assert.ok(metrics);
    assert.equal(metrics.promptTokens, 10_500);
    assert.equal(metrics.cachedInputTokens, 9_000);
    assert.equal(metrics.cacheWriteTokens, 500);
    assert.equal(metrics.hitPercent, (9_000 / 10_500) * 100);
  });

  it("combines cache usage across providers", () => {
    const metrics = cacheMetrics([
      { measured: { inputTokens: 100, cacheReadTokens: 300, cacheWriteTokens: 0 } },
      { measured: { inputTokens: 200, cacheReadTokens: 0, cacheWriteTokens: 100 } },
    ]);

    assert.ok(metrics);
    assert.equal(metrics.promptTokens, 700);
    assert.equal(metrics.hitPercent, (300 / 700) * 100);
  });

  it("does not show a rate before any prompt tokens are measured", () => {
    assert.equal(
      cacheMetrics([
        { measured: { inputTokens: 0, cacheReadTokens: 0, cacheWriteTokens: 0 } },
      ]),
      null
    );
  });
});
