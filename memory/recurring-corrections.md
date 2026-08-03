# Recurring Corrections

Track corrections that apply across the whole workspace.

## Corrections

- **Windows path equality** — `canonicalize()` yields `\\?\D:\…` while the UI often has a
  stripped display path. When comparing project roots (delete chat, active project), use
  `display_path` (or equivalent normalize) on both sides; never require byte-identical `PathBuf`s
  from the webview.
- **Deleting the open chat** — backend deletes then creates a fresh empty thread. UI must notify
  (toast) or the identical “Untitled chat” looks like a no-op.
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
