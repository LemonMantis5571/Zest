import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { isLongTurn, LONG_TURN_NOTIFICATION_MS } from "./notificationPolicy.ts";

describe("notification policy", () => {
  it("only marks turns at or above the long-turn threshold", () => {
    assert.equal(isLongTurn(LONG_TURN_NOTIFICATION_MS - 1), false);
    assert.equal(isLongTurn(LONG_TURN_NOTIFICATION_MS), true);
    assert.equal(isLongTurn(LONG_TURN_NOTIFICATION_MS + 1), true);
  });
});
