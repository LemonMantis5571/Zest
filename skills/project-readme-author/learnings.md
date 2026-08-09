# Learnings: project-readme-author

## Workspace facts

- No `uv` project, no `shared/select_operation.py`, no
  `mcp__image-tools__get_image_metadata`, no `project-logo-author` skill — use
  the fallback rules in `skill.md` and read PNG dimensions directly (Node
  script reading IHDR bytes at offsets 16/20).
- Rust workspace: tagline sync target is `[workspace.package].description` in
  root `Cargo.toml`, not `pyproject.toml`.
- `ask_user` is the local equivalent of `AskUserQuestion`.

## Zest README specifics (2026-02 apply)

- Logo: `assets/logo.png` is 1024×1024 → display `width="512"` per the retina
  rule. Path in README is `./assets/logo.png`.
- Repo `LemonMantis5571/Zest` returns HTTP 404 from the public GitHub API
  (private or not yet public) → **cannot verify stars/downloads**. Do not add
  star or download badges; keep only CI + license badges.
- README word count was 1457 → above the 500-word TOC threshold; TOC is
  required.
- GitHub strips emoji from heading anchors (`## 🚀 Quick Start` →
  `#quick-start`): TOC links must use emoji-free anchors.
- README has no social preview configured; repo settings need one (1280×640).
- The README is a Rust workspace app + CLI: treat as "CLI + desktop app"
  project type — terminal GIF aha ideal but no demo GIF exists yet; the
  mermaid diagram fills the visual slot until one is added.

## Recurring corrections to remember

- Never invent quantified metrics (stars, downloads, "10x faster") — the
  "verify all claims" rule beats the virality checklist.
- On `modify`, preserve custom prose; only touch dynamic content unless the
  user explicitly approves restructuring.
