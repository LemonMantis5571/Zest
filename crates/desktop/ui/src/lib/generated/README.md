# Generated desktop DTOs (ts-rs)

Rust is the source of truth for `ChatEvent` and `SessionInfo` wire shapes in
`crates/desktop/src/lib.rs`. Regenerate after changing those types:

```powershell
$env:CARGO_TARGET_DIR = (Resolve-Path .\target).Path
cargo test -p zest-desktop --features export-bindings --lib export_bindings
```

`TS_RS_EXPORT_DIR` is set in repo `.cargo/config.toml` to this directory.

Committed files:

- `ChatEvent.ts` — tagged union (`kind` + snake_case variants)
- `SessionInfo.ts` — camelCase session snapshot (`messages` as `unknown[]` in codegen;
  `../types.ts` narrows to `ChatMessage[]`)

`ChatMessage` / `ToolPart` stay handwritten in `../types.ts` (UI projection +
normalization). App code imports from `../types.ts`, which re-exports the
generated event/session types.
