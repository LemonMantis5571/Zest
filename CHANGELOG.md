# Changelog

Notable user-facing changes are recorded here. This is a release summary, not
a replacement for the commit history.

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
