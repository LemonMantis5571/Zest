# Validation guide

> Reconstructed from the `project-readme-author` skill spec (v2.5.1) because
> the original package's reference files are not available in this workspace.
> Every checklist item below is derivable from the skill's own rules; mark any
> extra items you add with a note.

## Scoring format

Weighted score across four tiers:

| Tier | Weight |
| --- | --- |
| Essential | 40% |
| Professional | 25% |
| Elite | 15% |
| Virality | 20% |

Each checklist item is pass/fail. Score = (passes / total) per tier, then
weighted sum. Report the per-tier breakdown plus overall score, and order
recommendations by tier (Essential first).

## Section ordering (critical, applies to all tiers)

Verify the Hook → Prove → Enable → Extend order:

1. Value proposition / "why" sections appear **before** feature lists.
2. Quick start appears **before** deep reference docs.
3. Extend content (contributing, community) comes **last**.
4. Hero (logo, badges, tagline, demo visual) is first and center-aligned.

Misordered sections undermine conversion regardless of content quality.

## Essential checklist (40%)

Non-negotiable basics.

- [ ] Title is exactly the repository name, original casing preserved
- [ ] Hero is center-aligned with `<div align="center">`
- [ ] Logo present, width = half actual pixel width (retina), with alt text
- [ ] 3–6 badges, shields.io style, working URLs
- [ ] Tagline: one sentence, ≤350 chars, bookend emoji pattern, explains the project
- [ ] Quick links present (docs / demo / community if they exist)
- [ ] Install or setup section with one-line command
- [ ] Working example / quick start under 10 minutes to first success
- [ ] License link present
- [ ] No broken links, no placeholder text, no "TODO"
- [ ] Heading hierarchy: one H1, logical H2/H3 nesting

## Professional checklist (25%)

Structure and clarity for real users.

- [ ] TOC present if README > 500 words
- [ ] Overview uses pain-point narrative (Pain / Solution / Result) or before/after table
- [ ] Features listed with emojis, 2–4 per section, scannable bullets
- [ ] Short paragraphs (3–5 lines max), one concept each
- [ ] Active voice, imperative mood, second person
- [ ] Configuration reference (options, env vars, files) if applicable
- [ ] Common tasks documented with commands
- [ ] Tiered CTAs: at least 3 of Try / Learn / Connect / Support / Contribute
- [ ] Custom prose preserved on modify; nothing removed without confirmation
- [ ] `<!-- custom -->` markers respected

## Elite checklist (15%)

Polish and depth.

- [ ] Aha moment: first visual after tagline shows transformation (GIF or strong demo)
- [ ] Architecture / how-it-works section (diagram or mermaid) for complex projects
- [ ] Comparison table vs alternatives with fair, linked benchmarks
- [ ] Quotable stats block (shareable `<div align="center">` table)
- [ ] Troubleshooting / FAQ for common failure modes
- [ ] Advanced usage section (power users)
- [ ] Social preview image configured (1280×640, logo + tagline + metric)
- [ ] Headless / API / integration reference for programmatic use

## Virality checklist (20%)

Share triggers and social proof.

- [ ] Curiosity hook in hero (question, stat, comparison, or challenge) — verifiable
- [ ] Star badge if stars > 100
- [ ] Download badge if downloads > 1000/week
- [ ] Quantified trust signals (downloads, stars, contributors) with real numbers
- [ ] Authority endorsements / testimonials with named sources
- [ ] "Used by" logos (6–12) if available
- [ ] Comparison wins table vs alternatives
- [ ] Tiered CTAs placed in multiple sections
- [ ] Shareable quotable stats block
- [ ] All claims verifiable — never fabricate metrics

## Project-type specifics

Run these in addition to the tiers above.

| Type | Extra checks |
| --- | --- |
| CLI | Terminal GIF after tagline; `--help`-style usage block; exit codes / env vars |
| Library | 3-line code example with "wow" output; API reference; version pinning |
| AI/ML | Benchmark chart; model compatibility table; quota/cost notes |
| Web app | Core-interaction GIF; deploy steps; environment variables |

## Report format

```
Overall: NN/100
  Essential:  NN/40
  Professional: NN/25
  Elite:     NN/15
  Virality:  NN/20

Top recommendations (by tier):
  1. [Essential] ...
  2. [Professional] ...
  ...
```
