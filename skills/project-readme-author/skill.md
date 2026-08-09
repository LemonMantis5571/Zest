# Skill: Project README Author

Create, modify, validate, and optimize `README.md` files following GitHub best
practices.

- **Author:** tsilva — **Version:** 2.5.1 — **License:** MIT
- **Compatibility:** any environment
- **Arguments:** `[create|modify|validate|optimize] [path]`
- **User-invocable:** true
- **Model invocation:** false (never auto-invokes a model)

> **Workspace adaptations (read first).** This skill was authored for a
> different toolchain. In this workspace:
>
> - `uv run shared/select_operation.py` and `uv run shared/detect_project.py`
>   do **not** exist — use the fallback rules in each section instead.
> - `mcp__image-tools__get_image_metadata` is unavailable — read the PNG header
>   directly (e.g. a small Node script that prints IHDR width/height).
> - The `project-logo-author` skill is not installed — if no `logo.png` exists,
>   either create one or skip the logo and say why; do not invent one.
> - `AskUserQuestion` maps to this workspace's `ask_user` tool.
> - There is no `pyproject.toml` — for Rust workspaces, the tagline source is
>   `[workspace.package].description` in the root `Cargo.toml`; sync crafted
>   taglines back there.

READMEs that hook readers in 5 seconds, prove value in 30 seconds, and enable
success in under 10 minutes.

## Operations

| Operation | Triggers | Purpose |
| --- | --- | --- |
| `create` | No README exists, "create/generate README" | Build from scratch |
| `modify` | README exists, "update/change README" | Preserve structure, update sections |
| `validate` | "check/review/audit README" | Score against best practices |
| `optimize` | "improve/enhance README" | Fix issues, enhance quality |

## Operation detection

Use the deterministic operation selector:

```
uv run shared/select_operation.py --skill project-readme-author --args "$ARGUMENTS" --check-files "README.md"
```

Fallback rules (if script unavailable — this workspace):

1. Check the request for explicit operation keywords.
2. Check if `README.md` exists at the target path.
3. Default: `create` if no README, `modify` if README exists.

## Create operation

Use when building a README from scratch. Follow the Core Framework and
Workflow sections below.

Mandatory pre-draft checklist:

- [ ] Aha moment identified — "What's the most impressive single thing?"
- [ ] Tagline crafted with emoji(s)
- [ ] At least one quantified metric (if available)
- [ ] Appropriate CTA tier determined

## Modify operation

Use when updating an existing README while preserving its structure.

- Keep custom prose — user-written descriptions, explanations, context.
- Update dynamic content — versions, badge URLs, install commands.
- Respect markers — content within `<!-- custom -->...<!-- /custom -->` is
  never touched.
- Preserve section order — don't reorder unless explicitly requested.
- Preserve manual notes — any hand-written note, warning, tip that's factually
  relevant.
- Default to preservation — when relevance is unclear, use `ask_user` to
  confirm.
- Never assume obsolescence — only remove when explicitly asked or factually
  incorrect.
- Deprecated sections — ask the user via `ask_user` before removing.
- When in doubt, preserve existing content and use `ask_user` to confirm
  before removing anything.

## Validate operation

Score an existing README against best practices. Run Essential → Professional →
Elite → Virality checklists plus project-type specifics. See
`references/validation-guide.md` for scoring format, tiers, project-type
checks, and checklists.

Scoring weights: **Essential 40%, Professional 25%, Elite 15%, Virality 20%.**

## Optimize operation

**Quick wins (auto-apply):** center hero, add alt text, fix badge URLs, add TOC
if >500 words, standardize badge style, fix heading hierarchy, add emojis to
headers.

**Virality quick wins (auto-apply):**

- Add star badge if stars > 100
- Add download badge if downloads > 1000/week
- Format existing stats as quotable block

**Requires approval:** add new sections, rewrite tagline, change badge
selection, remove emojis, restructure content order.

**Virality suggestions (require approval):**

- Add curiosity hook to hero
- Restructure overview as pain point narrative
- Create comparison table vs alternatives
- Add tiered CTAs

## Core framework: Hook → Prove → Enable → Extend

| Phase | Time | Purpose | Elements | Virality trigger |
| --- | --- | --- | --- | --- |
| Hook | 0–3 sec | Instant recognition | Logo + badges + one-liner + demo visual | Curiosity gap + visual impact |
| Prove | 3–30 sec | Build credibility | Social proof, features, trust signals | Social proof + comparison wins |
| Enable | 30 sec – 5 min | Immediate success | One-liner install + working example | "I can do this" moment |
| Extend | Committed users | Deep engagement | Docs links, contributing, API reference | Share triggers + community |

Goal: time to first success under 10 minutes. The first 5–10 visible lines
determine whether users stay or leave.

## Aha moment visualization

The "aha moment" is the single most impressive demonstration of the project's
value. It must answer "What does this DO?" within 3 seconds.

### 3-second rule

The first visual element after the tagline must show transformation —
before → action → after.

### Aha patterns by project type

| Type | Aha format | Example |
| --- | --- | --- |
| CLI | Terminal GIF: before → command → after | ripgrep searching 1M files in 0.2s |
| Library | 3-line code with commented "wow" output | # 50 lines → 3 lines |
| AI/ML | Benchmark comparison chart | "2x faster than GPT-3" |
| Web app | GIF of core interaction loop | One-click deploy animation |

### Aha requirements

- Show transformation — what changes from input to output.
- Max 5 seconds — attention drops sharply after this.
- Loop seamlessly — GIFs restart without jarring cuts.
- Placement — immediately after tagline, before any text.

### Identifying the aha moment

Ask "What's the most impressive single thing this project does?" then visualize
it.

## Logo generation (mandatory)

Every README must have a logo:

1. Check for an existing logo — look for `logo.png` at the repo root; if found,
   skip to README generation.
2. Generate if missing — invoke the project-logo-author skill (not installed in
   this workspace — see adaptations).
3. Determine display size — read the image width, then divide by 2 for retina
   display (e.g., 1024px → `width="512"`).

## Hero section

The hero must be center-aligned, with these elements in order:

### Title rule

The title must be exactly the repository name. Preserve original casing —
`my-awesome-tool` stays `my-awesome-tool`, not "My Awesome Tool".

```html
<div align="center">
  <img src="logo.png" alt="Project Name" width="{DISPLAY_WIDTH}"/>

  [![Build](badge)](link) [![Version](badge)](link) [![License](badge)](link)

  **A clear, catchy one-liner that explains what this does and why it matters**

  [Documentation](url) · [Demo](url) · [Discord](url)
</div>
```

### Curiosity hook (optional)

A bold line placed after badges, before the tagline, to create an information
gap:

| Type | Example |
| --- | --- |
| Question | "Ever spent 2 hours debugging what this fixes in 10 seconds?" |
| Stat | "Used by 50,000+ developers worldwide" |
| Comparison | "10x faster than grep for code search" |
| Challenge | "Find any file in your repo under 100ms" |

Rules: must be verifiable (don't exaggerate), connect to a real pain point,
create desire to learn more.

### Hero elements

| Element | Specification |
| --- | --- |
| Logo | Width = half actual pixels (for retina), centered |
| Badges | 3–6 maximum, shields.io for consistency |
| Curiosity hook | Optional bold line creating information gap |
| Tagline | One sentence with emoji(s), max 350 chars (fits GitHub "About" field) |
| Quick links | Docs, demo, community (if available) |

### Tagline rules

The tagline is THE hook — the single most critical line. It must be short,
witty, and instantly communicate what the project does.

Bookend emoji pattern: one emoji at START, one at END — visual framing that
draws the eye and reinforces the message from both sides.

Requirements:

- Max 350 characters — ideal 80–150 chars, punchy and scannable.
- Instantly clear — reader understands what the project does from this line
  alone.
- Source from `pyproject.toml` (`Cargo.toml` here) — if a description exists,
  use it as a base and enhance. If crafting new, sync back.

Good taglines (bookend pattern):

- ✅ 🚀 Build production-ready APIs in minutes, not hours ⚡
- ✅ 🔍 Find anything in your codebase instantly 🎯
- ✅ 🎨 Turn designs into code with one command 💻
- ✅ 🔧 Magnificent app which corrects your previous console command ✨

Anti-patterns:

- ❌ "A Python library for doing Y" — no emojis, too generic
- ❌ "⚡ Fast and easy to use" — vague, doesn't explain what it does
- ❌ "Tool for developers" — meaningless, could be anything
- ❌ "🚀 🔥 ⚡ Super awesome project 💪 🎉" — emoji spam, no substance

### GIF demo placement

For CLI tools, place an animated GIF demo immediately after the tagline.

## Pain point narrative

Structure the Overview section using Problem–Solution–Result for emotional
connection:

- **The Pain:** 1–2 sentences describing the frustration users face.
- **The Solution:** what this project does differently.
- **The Result:** quantifiable outcome — time saved, lines reduced, speed gained.

Before/after format (alternative):

| Before | After |
| --- | --- |
| 50 lines of boilerplate | 3-line function call |
| 2 hours debugging | 10-second fix |
| Manual deployments | One-click CI/CD |

## Social proof hierarchy

Ordered by impact (include what you have):

1. Quantified trust signals — `> **50,000+** downloads | **4,000+** stars | **500+** contributors`
2. Authority endorsements — "This tool is incredible." — @notable_person, CTO at Company
3. "Used by" logos — 6–12 recognizable company logos
4. Community size — Discord badge with member count

Rules: always quantify (not "many users" but "50,000+ users"), verify all
claims, update quarterly.

## Tiered CTA system

Provide multiple engagement paths from low to high commitment:

| Level | Action | Example |
| --- | --- | --- |
| 1. Try | Quick start | `npx create-myapp` or `pip install myapp` |
| 2. Learn | Documentation | "Read the docs (5 min read)" |
| 3. Connect | Community | "Questions? Join our Discord" |
| 4. Support | Star | "Useful? Give us a star ⭐" |
| 5. Contribute | PR | "Good first issues" |

Include at least 3 tiers. Place the primary CTA (Try) prominently; others in
appropriate sections.

## Shareable elements

Elements designed to be screenshot and shared:

**Quotable stats block:**

```html
<div align="center">

| Metric | Value |
|--------|-------|
| ⚡ Speed | 10x faster than alternatives |
| 📦 Size | 2MB (no dependencies) |
| 🔧 Setup | 30 seconds |

</div>
```

**Comparison tables** — fair benchmarks against alternatives:

| Tool | Speed | Memory | Features |
| --- | --- | --- | --- |
| This project | 0.2s | 50MB | ✅ All |
| Alternative A | 2.1s | 200MB | ⚠️ Partial |
| Alternative B | 1.5s | 150MB | ❌ Missing |

Rules: use equivalent configurations, link to benchmark methodology, update
when alternatives improve.

## Social preview

Remind users to configure GitHub's social preview image (Settings → Social
preview): 1280×640px, include logo, tagline, key metric. Appears when the repo
is shared on social media.

## Badges

Use 4–7 badges in priority order: Build/CI → Coverage → Version → License →
Downloads → Community. For badge implementation details and code, see
`references/badges-and-visuals.md`.

## Writing style

- Active voice, imperative mood: "Install the package", not "The package can
  be installed".
- Second person, present tense: "You can configure…" with contractions for a
  conversational tone.
- Short paragraphs: max 3–5 lines, one concept per paragraph.
- Emojis: use liberally on section headers (🚀 Quick Start), feature bullets
  (⚡ Fast), status indicators (✅ Done), and CTAs (⭐ Star us!). 2–4 per
  section, never in code blocks.

## README by project type

For detailed templates and examples by project type (AI/ML, CLI, Libraries,
Web apps), see `references/project-types.md`. For visual elements, social
proof, and community links, see `references/badges-and-visuals.md`.

## Workflow

### Create workflow

1. Detect project type — `uv run shared/detect_project.py --path "$(pwd)"`
   (not available here: infer from the codebase — CLI vs library vs app).
2. Extract metadata — name, description, version, author, license. Use the
   `Cargo.toml`/`package.json` description as the tagline base (add emojis,
   preserve core message). If no description, write a crafted tagline back.
3. Check/generate logo — look for `logo.png`, generate with the
   project-logo-author skill if missing (not installed here).
4. Calculate display width — half actual pixel width for retina.
5. Generate README.md — following Hook → Prove → Enable → Extend.

### Modify workflow

1. Read the existing README, identify sections, detect custom content.
2. Confirm uncertain deletions via `ask_user`.
3. Apply requested changes (follow tagline sync rules).
4. Validate result — no broken links or formatting.

### Validate workflow

Run Essential → Professional → Elite → Virality checklists plus project-type
specifics. Calculate the weighted score (Essential 40%, Professional 25%,
Elite 15%, Virality 20%), generate a report with actionable recommendations.
See `references/validation-guide.md`.

Critical: check section ordering against Hook → Prove → Enable → Extend. Verify
the value proposition/"why" sections appear before feature lists, quick start
appears before deep reference docs, and extend content (contributing,
community) comes last. Misordered sections undermine conversion regardless of
content quality.

### Optimize workflow

1. Run validation to identify issues.
2. Apply quick wins (safe auto-fixes).
3. Present suggestions requiring approval (follow tagline sync rules).
4. Re-validate and show improvement.
