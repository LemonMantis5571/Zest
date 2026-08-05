# Recurring Corrections

Track corrections that apply across the whole workspace.

## Corrections

- **User-facing copy** — describe the user's outcome and next action, not Zest's
  implementation. Do not expose workers, chat shells, local gateway ports, script paths,
  cache internals, or raw debugging details in the UI; keep only technical information users
  need to configure or recover their work.
- **Visual language** — keep Zest UI chrome near-black/charcoal with lavender and neutral
  status accents; do not introduce green as a product accent.
- **Context handling** — compaction is automatic after the threshold; do not expose manual
  compact buttons or tell users to compact from the UI when context limits are reached.
- **The gateway decision is closed** — Zest bundles the pinned CLIProxyAPI executable as a Tauri
  sidecar and does not implement native subscription OAuth. Do not restate this as an open
  single-binary decision. Revisit only for documented native OAuth/API access, API-key billing, or
  a demonstrated sidecar security/reliability failure; replacements happen per provider behind
  `Provider`, never as another monolithic gateway.
- **Windows path equality** — `canonicalize()` yields `\\?\D:\…` while the UI often has a
  stripped display path. When comparing project roots (delete chat, active project), use
  `display_path` (or equivalent normalize) on both sides; never require byte-identical `PathBuf`s
  from the webview.
- **Deleting the open chat** — backend deletes then switches to an unsaved empty draft. Do not
  create a new history row until the first message is sent; the UI should say that no new chat
  was saved so the deletion is unambiguous.
- **Empty system prompt** — `.zest/system.md` empty/missing is fine; base `DEFAULT_SYSTEM` still
  applies. Gateway down (`127.0.0.1:8317`) is a separate failure mode.
- **WebView menus** — do not use Base UI Menu/Portal popovers; they have crashed the Tauri
  WebView. Use positioned panels (model picker, composer +, confirm dialogs).
- **Smart App Control (Windows)** — can block unsigned `build-script-build.exe` under `target/`.
  Fix is OS policy (SAC Off + reboot), not an in-repo code change.
- **Antigravity / Gemini fake 429s** — Upstream Antigravity can return
  `429 RESOURCE_EXHAUSTED` that is **not** real quota. Controlled A/B (same token, model,
  user message; only the system identity sentence changes) shows fingerprint filtering on
  some product identity phrases (reproduced for Hermes/Nous; OpenClaw base identity and
  neutral prompts succeeded). Confirmed against the native Antigravity endpoint without
  CLIProxyAPI in the path — see
  [CLIProxyAPI#4696](https://github.com/router-for-me/CLIProxyAPI/issues/4696).
  When Gemini/Antigravity 429s appear: check system-prompt identity wording and ledger/
  cooldown labeling before treating it as spent quota. Do not add aggressive product-identity
  openers for Antigravity-backed models without verifying; prefer neutral coding-agent framing.
- **pdf-inspector / lopdf RustSec** — crates.io `pdf-inspector` 0.1.7 pins `lopdf ^0.41`
  (RUSTSEC-2026-0187). Desktop depends on the patched git rev from
  [firecrawl/pdf-inspector#222](https://github.com/firecrawl/pdf-inspector/pull/222) until a
  crates.io release ships; switch back to a versioned crates.io dep when available.
- **Gateway Claude/Codex “Signed in” ≠ working** — CLIProxyAPI can leave a cooldown or
  stub auth file that looks Ready locally while chat gets 503 `auth_unavailable`.
  Builds do not wipe `~/.cli-proxy-api`. Always probe before opening a gateway chat;
  reject near-empty stubs (< ~200 bytes), but do **not** treat Claude’s ~400-byte
  OAuth files as incomplete (Codex files are multi-KB). Surface Connect again.
  Optional gateway `disable-cooling: true` reduces black-hole after transient 503s.
