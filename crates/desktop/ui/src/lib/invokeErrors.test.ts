import assert from "node:assert/strict";
import test from "node:test";

import { shouldOfferProviderReconnect } from "./invokeErrors.ts";

test("provider reconnect detection follows the desktop error contract", () => {
  assert.equal(
    shouldOfferProviderReconnect("DeepSeek needs to be reconnected. Try again."),
    true
  );
  assert.equal(
    shouldOfferProviderReconnect(
      "{\"code\":\"auth_unavailable\",\"message\":\"No sign-in is available\"}"
    ),
    true
  );
  assert.equal(shouldOfferProviderReconnect("This provider is overloaded."), false);
});
