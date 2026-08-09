# Badges and visuals

> Reconstructed from the `project-readme-author` skill spec (v2.5.1) because
> the original package's reference files are not available in this workspace.

## Badges

Use **4–7 badges** in priority order:

1. Build/CI
2. Coverage
3. Version
4. License
5. Downloads
6. Community

Use shields.io for consistency. One badge = one line of markdown in the hero.

### Implementation

**GitHub Actions CI:**

```markdown
[![CI](https://github.com/OWNER/REPO/actions/workflows/ci.yml/badge.svg)](https://github.com/OWNER/REPO/actions/workflows/ci.yml)
```

**Shields.io static:**

```markdown
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Version](https://img.shields.io/badge/version-1.2.3-blue.svg)](https://github.com/OWNER/REPO/releases)
```

**Version from registry:**

```markdown
[![crates.io](https://img.shields.io/crates/v/CRATE.svg)](https://crates.io/crates/CRATE)
[![npm](https://img.shields.io/npm/v/PACKAGE.svg)](https://www.npmjs.com/package/PACKAGE)
```

**Downloads:**

```markdown
[![Downloads](https://img.shields.io/crates/d/CRATE.svg)](https://crates.io/crates/CRATE)
```

**Community:**

```markdown
[![Discord](https://img.shields.io/discord/SERVER_ID.svg)](https://discord.gg/INVITE)
```

### Rules

- Verify every badge URL before shipping; broken badges are worse than none.
- Do not add a star badge unless stars > 100; a download badge unless
  downloads > 1000/week. Never fabricate counts.
- Prefer badges that resolve to real status (CI) over decorative ones.

## Visual elements

### Hero

Center-aligned `<div align="center">` with, in order: logo, badges, optional
curiosity hook, tagline, quick links. Logo width = half actual pixels for
retina (1024px source → `width="512"`).

### Quotable stats block

Shareable, screenshot-friendly:

```html
<div align="center">

| Metric | Value |
|--------|-------|
| ⚡ Speed | 10x faster than alternatives |
| 📦 Size | 2MB (no dependencies) |
| 🔧 Setup | 30 seconds |

</div>
```

Only include metrics that are true and verifiable.

### Comparison tables

Fair benchmarks against alternatives. Rules: equivalent configurations, link
to benchmark methodology, update when alternatives improve. Use ✅ / ⚠️ / ❌
columns for feature parity.

### GIF demos

For CLI tools, an animated terminal GIF goes immediately after the tagline.
Requirements: shows before → action → after; max 5 seconds; loops seamlessly.

### Social preview

GitHub Settings → Social preview: 1280×640px image with logo, tagline, and a
key metric. This is what renders when the repo is shared on social media.
Add a reminder in `learnings.md` when the repo lacks one.

### Social proof

Ordered by impact: quantified trust signals → authority endorsements →
"used by" logos → community size. Always quantify and verify; update
quarterly.
