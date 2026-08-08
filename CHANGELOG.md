# Changelog

Notable user-facing changes are recorded here. This is a release summary, not
a replacement for the commit history.

## Unreleased

### Added

- Usage screen (`Ctrl+Shift+U`, or "Full report" on the profile) with a 7/30/90
  day window, daily spend stacked by provider, and a per-model breakdown.
- Per-model attribution in the usage ledger. Turns are billed to the model the
  endpoint actually served, so a substitution is costed against what ran.
- A local price book at `<data dir>/zest/prices.toml`, seeded once and never
  rewritten by Zest. Models with no rate are reported as unpriced rather than
  free, and the share of tokens that could be costed is shown alongside every
  dollar figure.
- `zest usage` now prints the last 30 days with its own coverage line.
- Rates come from the published LiteLLM table, cached for a day next to the
  ledger with an offline fallback, so thousands of models price without any
  hand-maintenance. `prices.toml` is now purely an override layer and always
  wins.
- Usage read back from Claude Code's and Codex's own on-disk transcripts, so
  turns run in those CLIs directly are counted even though Zest never sent them.
  Parsing is cached per file, making a re-scan of a gigabyte of transcripts
  effectively free.

### Notes

- Cost figures are an estimate at list API rates, not a bill. Zest has no
  billing relationship with any provider, and a subscription does not charge at
  these rates. Where a CLI records what it was actually charged, that figure is
  used instead and labelled as reported.
- Refreshing rates is an unauthenticated GET of one public file. Nothing about
  your usage is sent anywhere, and transcripts are only ever read from disk.
- Days recorded before this release have real token totals but no model to
  attribute them to; they appear as uncosted rather than being backfilled.

## 0.1.0 beta - 2026-08-05

### Added

- Windows-first Tauri desktop app and terminal front-end sharing one Rust core.
- Bundled, pinned CLIProxyAPI sidecar with gateway verification and MSI/NSIS
  packaging helpers.
- OS credential-manager setup for OpenAI-compatible API endpoints, including
  DeepSeek, OpenAI, and local servers; native Anthropic keeps its environment
  variable configuration.
- ACP/headless delegation to already-authenticated Claude Code and Gemini CLI
  workers, with isolated Git worktrees and approval boundaries.
- Workbench activity and outline views, forked conversations, checkpoints, and
  automatic context compaction.
- JSONL/headless protocol for editor and CI integrations.
- Release checksums, security guidance, contributor documentation, and an
  explicit MIT license.

### Beta limitations

- Windows is the primary supported desktop platform.
- Approved shell commands are not OS-sandboxed.
- Provider usage may be estimated or unavailable when an endpoint does not
  report token counts.
- Configuration and headless protocol details may change before 1.0.
