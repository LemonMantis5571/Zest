# Plan 010: Keep the optional music plugin discoverable

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan in
> `plans/README.md` unless a reviewer dispatches you and maintains the index.
>
> **Drift check (run first)**: `git diff --stat 1e37803..HEAD -- crates/desktop/ui/src/components/NowPlayingButton.tsx crates/desktop/ui/src/components/TopbarPanel.tsx crates/desktop/ui/src/lib/api.ts crates/desktop/ui/src/lib/backend.ts`
> This plan depends on plan 008 if both are being implemented in the same
> worktree; resolve any overlap before editing.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: `plans/008-serialize-now-playing.md`
- **Category**: dx
- **Planned at**: commit `1e37803`, 2026-08-13

## Why this matters

The music add-on is optional and installed separately, but its topbar entry
currently disappears when the add-on is missing or not ready. That leaves no
visible way to discover the feature or open the plugin folder. The topbar
should always offer a small music entry; clicking it can explain the state and
provide the folder/refresh action without pretending that the add-on is part of
the official build.

## Current state

- `crates/desktop/ui/src/components/NowPlayingButton.tsx:20-84` discovers the
  `now-playing` plugin and polls for it when unavailable.
- `crates/desktop/ui/src/components/NowPlayingButton.tsx:122` returns `null`
  when `checked` is false or `plugin?.available` is false. This hides the
  entire icon when the optional plugin is absent or invalid.
- `crates/desktop/ui/src/components/NowPlayingButton.tsx:126-145` renders an
  inline artwork/title/artist trigger only after an available plugin exists.
- `crates/desktop/ui/src/components/SettingsPanel.tsx:990-1065` already offers
  Open folder, Refresh, and enable/disable actions for Extras.
- `crates/desktop/ui/src/lib/api.ts:111-120` already exposes
  `listPlugins`, `openPluginsFolder`, and `setPluginEnabled`; do not add a new
  installation mechanism here.
- `README.md` and `docs/PLUGINS.md` state that optional plugins are not bundled
  in the official release and are installed in `%LOCALAPPDATA%\\Zest\\plugins`.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| UI tests | `npm run ui:test` | all tests pass |
| UI lint | `npm run ui:lint` | exit 0, no warnings |
| UI build | `npm run ui:build` | exit 0 |
| Final verification | `npm run verify` | exit 0 |

## Scope

**In scope**:

- `crates/desktop/ui/src/components/NowPlayingButton.tsx`
- A small existing UI helper only if needed for the empty/unavailable state.
- `crates/desktop/ui/src/lib/fixtureBackend.ts` or its tests only if fixture
  behavior must cover the new state.
- `plans/README.md` status row.

**Out of scope**:

- Bundling the plugin in the official release.
- Creating a runtime installer, marketplace, signature system, or sandbox.
- Changing plugin discovery or manifest validation in Rust.
- Changing Now Playing async ordering from plan 008.
- Redesigning the whole topbar; plan 011 owns responsive sizing.

## Steps

### Step 1: Render a stable topbar entry

Keep the `TopbarPanel` mounted after the initial plugin discovery check even
when the plugin is absent or unavailable. Use the music icon as the stable
fallback trigger. When a track is available, preserve the requested compact
inline text trigger: artwork when present, then `title · artist`; the full
track details and controls remain inside the panel.

Do not show a misleading “Turn on” action when no valid plugin was discovered.
The fallback panel should say, in short non-technical copy, that Music is an
optional add-on and provide “Open folder” and “Refresh” actions. If the plugin
exists but is unavailable, show its short detail and the same recovery actions.

**Verify**: `npm run ui:lint` → exit 0.

### Step 2: Keep optional-state transitions correct

Use the existing `openPluginsFolder` and `listPlugins` backend methods. After a
folder open or refresh, update the panel state without requiring a Settings
visit. Preserve the existing enable/disable behavior when the plugin is
available. Keep polling/discovery bounded and stop it on unmount; do not start
media polling unless the plugin is enabled and available.

Ensure the panel stays usable before the first discovery result and while a
refresh is pending. Keep copy concise: “Music add-on not found”, “Open folder”,
“Refresh”, and “Turn on” are sufficient states.

**Verify**: `npm run ui:test` → all tests pass.

### Step 3: Cover fallback and available states

Extend the fixture backend or add a small pure state helper test for:

1. no plugin discovered: stable icon/panel with Open folder and Refresh;
2. plugin discovered but not ready: detail and recovery action;
3. plugin available but disabled: Turn on action;
4. plugin enabled with track: artwork/title/artist trigger and controls; and
5. refresh after installing a plugin changes the panel without restarting Zest.

Do not introduce a browser test framework for this; use the existing UI test
style or keep state decisions pure and testable.

**Verify**: `npm run ui:test` and `npm run ui:build` → exit 0.

### Step 4: Run the full gate

Run the repository verification command.

**Verify**: `npm run verify` → exit 0.

## Test plan

- Reuse the fixture plugin behavior in
  `crates/desktop/ui/src/lib/fixtureBackend.ts:351-367`.
- Test the public states, not implementation-specific class names.
- Manually verify on a clean profile with no `%LOCALAPPDATA%\\Zest\\plugins`
  folder and with the sample plugin installed.

## Done criteria

- [ ] A music icon remains visible after discovery reports no plugin.
- [ ] The fallback panel clearly says the add-on is optional.
- [ ] Open folder and Refresh are usable without opening Settings.
- [ ] Available plugins still support Turn on/Turn off and the full music card.
- [ ] No media polling runs for a missing, unavailable, or disabled plugin.
- [ ] Tests cover all five plugin states above.
- [ ] `npm run verify` exits 0.
- [ ] no files outside Scope are modified.
- [ ] `plans/README.md` marks plan 010 `DONE` only after implementation.

## STOP conditions

- Making the entry visible requires bundling or auto-installing the optional
  plugin.
- The backend lacks a safe folder-open command and adding one would require a
  new permission or filesystem contract.
- The implementation would duplicate the race fix from plan 008.
- A verification command fails twice after a focused correction.

## Maintenance notes

- Keep this entry stable for future optional plugins if the topbar gains an
  Extras hub; do not create one topbar button per optional add-on without a
  product decision.
- Keep the distinction between “not found”, “not ready”, “off”, and “on” so
  support can tell installation problems from media-session problems.
