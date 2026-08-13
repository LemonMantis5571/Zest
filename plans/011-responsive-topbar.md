# Plan 011: Make the chat topbar responsive

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan in
> `plans/README.md` unless a reviewer dispatches you and maintains the index.
>
> **Drift check (run first)**: `git diff --stat 1e37803..HEAD -- crates/desktop/ui/src/components/ChatScreen.tsx crates/desktop/ui/src/components/TopbarPanel.tsx crates/desktop/ui/src/components/NowPlayingButton.tsx`
> This plan is intentionally last among the five because the stable optional
> plugin entry from plan 010 changes the right-side content.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED
- **Depends on**: `plans/010-stable-plugin-entry.md`
- **Category**: bug
- **Planned at**: commit `1e37803`, 2026-08-13

## Why this matters

The chat header contains a flexible identity block and several controls: quota,
music, command palette, workbench, and settings. On narrow windows or with a
long profile/workspace/track label, the two flex groups can compete for width,
causing controls or the first part of the header to be clipped. The header
needs explicit shrink/grow rules and compact trigger states while preserving
the full text inside each opened panel.

## Current state

- `crates/desktop/ui/src/components/ChatScreen.tsx:913-943` renders a
  `justify-between` header. The left group has no `min-w-0`/`flex-1`, and the
  right group has no `shrink-0` or width cap.
- `crates/desktop/ui/src/components/ChatScreen.tsx:929-934` truncates the
  path line, but the parent identity block can still compete with the right
  controls before that truncation helps.
- `crates/desktop/ui/src/components/TopbarPanel.tsx:66-88` gives a text
  trigger `max-w-[230px]` and a panel width of 330 px. The panel already caps
  itself to the viewport, but the trigger participates in header width.
- `crates/desktop/ui/src/components/NowPlayingButton.tsx:126-145` uses
  artwork/title/artist as the music trigger when a track is available.
- The visual direction in `DESIGN.md` and `memory/decisions.md` favors compact,
  dark, near-black surfaces with lavender accents. Do not add green states or a
  second visual language.
- The product requirement is: the music trigger may be compact in the topbar;
  clicking it reveals controls, volume, artwork, title, and artist.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| UI tests | `npm run ui:test` | all tests pass |
| UI lint | `npm run ui:lint` | exit 0, no warnings |
| UI build | `npm run ui:build` | exit 0 |
| Final verification | `npm run verify` | exit 0 |

## Scope

**In scope**:

- `crates/desktop/ui/src/components/ChatScreen.tsx`
- `crates/desktop/ui/src/components/TopbarPanel.tsx`
- `crates/desktop/ui/src/components/NowPlayingButton.tsx` only for trigger
  sizing/class composition.
- A focused responsive CSS/token adjustment in
  `crates/desktop/ui/src/index.css` only if utility classes cannot express the
  breakpoint behavior.
- `plans/README.md` status row.

**Out of scope**:

- Replacing the topbar panel with a portal.
- Moving quota or music to another screen.
- Changing quota fetching behavior from plan 009.
- Changing music async behavior from plan 008.
- Changing sidebar/workspace selector layout.
- Removing accessible labels or making controls icon-only at every width.

## Steps

### Step 1: Give the header explicit flex ownership

Make the left identity block `min-w-0 flex-1`, make the avatar and text obey
shrink rules, and make the right control group `shrink-0` with a bounded
layout. Ensure the path line keeps `truncate` and cannot force the header
wider than its parent.

Do not solve overflow with a global `overflow-hidden` that silently hides
buttons. The controls must remain reachable by keyboard and pointer.

**Verify**: `npm run ui:lint` → exit 0.

### Step 2: Add narrow-window trigger behavior

At a documented narrow breakpoint, make the music trigger icon-first or
icon-only while retaining its accessible label and full text inside the panel.
Keep quota, command, workbench, and settings controls visible. If space is
still insufficient, use compact spacing and trigger classes before hiding any
non-essential text. Never hide the settings or command controls without an
accessible alternative.

Update `TopbarPanel` sizing only as needed: the panel must remain within the
viewport, and its trigger must be allowed to shrink without causing the right
group to overflow.

**Verify**: `npm run ui:build` → exit 0.

### Step 3: Validate the real layout states

Use the existing desktop UI and fixture backend to inspect these states at
minimum widths of 260 px, 360 px, 640 px, and a normal desktop width:

1. long profile name and long workspace path;
2. no music plugin, showing the stable music icon from plan 010;
3. long music title and artist;
4. quota panel open; and
5. music panel open with controls and volume.

Check that opened panels remain usable, the first identity line is not clipped
by the header edge, and all icon buttons have visible focus and accessible
labels. Capture screenshots or use the app's visual inspection workflow if
available; do not rely solely on a successful build.

**Verify**: `npm run ui:test` → all tests pass; manual layout checks show no
horizontal overflow at the widths above.

### Step 4: Run the full gate

Run lint, build, tests, and repository verification.

**Verify**: `npm run verify` → exit 0.

## Test plan

- Add a pure class/state test only if the breakpoint logic is extracted; do not
  add a browser automation dependency just for this header.
- Manual visual verification is required because the defect is geometric and
  the current test suite has no layout engine.
- Use the existing near-black/lavender tokens and `TopbarPanel` pattern when
  comparing screenshots.

## Done criteria

- [ ] Header has no horizontal overflow at 260 px, 360 px, 640 px, or normal
      desktop width.
- [ ] Long identity/path text truncates without pushing controls away.
- [ ] Music trigger remains discoverable when the optional plugin is missing.
- [ ] Music text can compact at narrow widths while full details remain in the
      opened panel.
- [ ] Quota, command palette, workbench, settings, and panel focus remain
      reachable and labeled.
- [ ] Manual visual checks are recorded in the implementation handoff.
- [ ] `npm run verify` exits 0.
- [ ] no files outside Scope are modified.
- [ ] `plans/README.md` marks plan 011 `DONE` only after implementation.

## STOP conditions

- The fix requires redesigning the sidebar or changing the panel portal
  decision.
- A breakpoint would hide a required control without an accessible alternative.
- The layout still overflows after two focused iterations; stop and report the
  smallest reproducible viewport and screenshot instead of adding arbitrary
  negative margins.
- A verification command fails twice after a focused correction.

## Maintenance notes

- New topbar actions must join the right-side bounded group and provide a
  compact trigger state before they are added.
- Reviewers should test both no-plugin and long-track states; the stable music
  entry is intentionally part of the width budget.
