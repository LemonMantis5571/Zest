import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { nowPlayingPluginState } from "./nowPlayingPluginState.ts";
import type { PluginView } from "./types.ts";

const plugin: PluginView = {
  id: "now-playing",
  name: "Now Playing",
  description: "See and control your music.",
  enabled: true,
  available: true,
  detail: "Ready",
};

describe("now playing plugin states", () => {
  it("keeps the entry in a checking state before discovery completes", () => {
    assert.equal(nowPlayingPluginState(false, null), "checking");
  });

  it("keeps a missing plugin discoverable", () => {
    assert.equal(nowPlayingPluginState(true, null), "missing");
  });

  it("separates an installed but unavailable plugin from a missing one", () => {
    assert.equal(
      nowPlayingPluginState(true, { ...plugin, available: false, detail: "Not ready" }),
      "unavailable"
    );
  });

  it("keeps an available plugin visibly off until the user turns it on", () => {
    assert.equal(nowPlayingPluginState(true, { ...plugin, enabled: false }), "disabled");
  });

  it("moves from the missing state to ready after refresh finds the add-on", () => {
    assert.equal(nowPlayingPluginState(true, null), "missing");
    assert.equal(nowPlayingPluginState(true, plugin), "ready");
  });
});
