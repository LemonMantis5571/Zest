import assert from "node:assert/strict";
import test from "node:test";

import {
  conversationRecovery,
  shouldOfferProviderReconnect,
} from "./invokeErrors.ts";

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

test("provider-owned recovery exposes an explicit copy target", () => {
  const recovery = conversationRecovery(
    JSON.stringify({
      code: "provider_unavailable",
      message: "Codex is not configured for this project.",
      details: {
        threadId: "thread-codex",
        providerId: "codex",
        providerLabel: "Codex",
        configured: false,
        availableProviders: [
          { id: "deepseek", label: "DeepSeek", model: "deepseek-chat" },
        ],
      },
    })
  );

  assert.deepEqual(recovery, {
    kind: "owner_unavailable",
    threadId: "thread-codex",
    providerId: "codex",
    providerLabel: "Codex",
    configured: false,
    providers: [{ id: "deepseek", label: "DeepSeek", model: "deepseek-chat" }],
  });
});

test("legacy recovery requires a provider choice", () => {
  const recovery = conversationRecovery(
    JSON.stringify({
      code: "thread_provider_unknown",
      message: "This chat has no provider owner.",
      details: {
        threadId: "thread-legacy",
        availableProviders: [{ id: "codex", label: "Codex", model: "gpt-5" }],
      },
    })
  );

  assert.deepEqual(recovery, {
    kind: "unknown_owner",
    threadId: "thread-legacy",
    providers: [{ id: "codex", label: "Codex", model: "gpt-5" }],
  });
});
