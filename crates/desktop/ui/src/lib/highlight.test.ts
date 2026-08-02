import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { highlightCode, languageLabel, normalizeLang } from "./highlight.ts";

describe("normalizeLang", () => {
  it("maps aliases", () => {
    assert.equal(normalizeLang("js"), "javascript");
    assert.equal(normalizeLang("TS"), "typescript");
    assert.equal(normalizeLang("text"), "plaintext");
  });
});

describe("languageLabel", () => {
  it("shortens common langs", () => {
    assert.equal(languageLabel("javascript"), "js");
    assert.equal(languageLabel("plaintext"), "text");
  });
});

describe("highlightCode", () => {
  it("emits inline color styles for javascript", async () => {
    const html = await highlightCode("const x = 1;\nfunction foo() {}", "js");
    assert.match(html, /style="color:#[0-9A-Fa-f]{6}"/);
    assert.match(html, /const/);
  });
});
