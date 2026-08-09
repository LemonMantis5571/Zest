# READMEs by project type

> Reconstructed from the `project-readme-author` skill spec (v2.5.1) because
> the original package's reference files are not available in this workspace.
> These are templates; adapt the section names to the project.

Every type follows Hook → Prove → Enable → Extend. Only the aha format, the
enabling example, and the extra sections change.

## CLI tools

Aha format: **terminal GIF** — before → command → after (e.g. ripgrep searching
1M files in 0.2s).

```markdown
<div align="center">
  logo + badges

  **🚀 One-liner tagline with bookend emojis ⚡**

  ![Demo GIF](./demo.gif)
</div>

## Why this exists
Pain / Solution / Result

## Install
one-line install command

## Quick start
usage block showing before → after

## Usage
- common commands
- configuration / env vars
- exit codes

## Documentation · Contributing · License (links)
```

Extra checks: `--help`-style usage block; exit codes; env vars; shell
completion; CI badge.

## Libraries

Aha format: **3-line code with a commented "wow" output** — # 50 lines → 3 lines.

```markdown
<div align="center">
  logo + badges

  **📦 One-liner tagline with bookend emojis 🚀**
</div>

## Why this exists
Pain / Solution / Result

## Install
pip/npm/cargo install ...

## Usage
```python
import awesome
result = awesome.do(input)  # what used to take 50 lines
print(result)  # 'wow' output
```

## API reference
table of functions/types

## Documentation · Contributing · License (links)
```

Extra checks: API reference; version pinning; feature flags; type hints/signatures.

## AI/ML

Aha format: **benchmark comparison chart** — "2x faster than GPT-3".

```markdown
<div align="center">
  logo + badges

  **🤖 One-liner tagline with bookend emojis ⚡**
</div>

## Why this exists
Pain / Solution / Result

## Benchmarks
chart/table vs alternatives, with methodology link

## Install
pip install ...

## Quick start
minimal working example

## Models
compatibility table (model → supported features)

## Cost & quota
honest cost notes

## Documentation · Contributing · License (links)
```

Extra checks: benchmark methodology link; model compatibility table; cost and
quota notes; reproducibility (seeds, versions).

## Web apps

Aha format: **GIF of the core interaction loop** — one-click deploy animation.

```markdown
<div align="center">
  logo + badges

  **🌐 One-liner tagline with bookend emojis ⚡**

  ![App demo](./demo.gif)
</div>

## Why this exists
Pain / Solution / Result

## Features
emojified bullets

## Quick start
- hosted demo link
- `npx create-myapp` / one-command setup

## Deploy
platform steps, env vars

## Configuration
env var table

## Documentation · Contributing · License (links)
```

Extra checks: hosted demo; deploy steps; environment variables table; auth
notes; screenshots per key screen.
