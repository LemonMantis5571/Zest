# Plan 004: Optimize custom PFP — small JPEG file, not fat JSON

> Revised 2026-08-02: keep file picker; resize/compress in UI; persist
> `~/.zest/avatar.jpg` (not a multi-MB data-URL in `user-profile.json`).

## Status

- **Priority**: P2
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: tech-debt
- **Planned at**: commit `2e0c60b`, 2026-08-02 (implemented on WIP)

## Intent (implemented)

- Keep header avatar button → Settings → User.
- Keep display name in `user-profile.json`.
- On pick: UI `optimizeAvatarFile` → 128px JPEG.
- On save: Rust writes `avatar.jpg` (max ~48KB); JSON has only `displayName`.
- On get: read file → data URL for `<img>`.
- Empty avatar data URL clears the file.
