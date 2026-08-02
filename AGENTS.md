# AI Operating Rules

This workspace is agent-agnostic. Any AI assistant can use it by reading these files before performing work.

## How to Use This Workspace

1. Read this file first.
2. Read `PROJECT_CONTEXT.md`.
3. Read relevant files inside `context/`.
4. Choose the most relevant skill inside `skills/`.
5. Check that skill's `learnings.md` before answering.
6. Produce the requested output.
7. When the user corrects something reusable, suggest adding it to the relevant `learnings.md` file.

## Global Rules

- Be direct and practical.
- Prefer complete, usable outputs over partial suggestions.
- Preserve existing project structure unless the user asks for refactoring.
- Do not remove existing filters, rules, or validations unless explicitly requested.
- State assumptions when information is missing.
- Do not expose secrets, credentials, tokens, private keys, or sensitive data.
- Use project terminology from `context/glossary.md` when available.
- Check `memory/recurring-corrections.md` before finalizing technical work.

## Output Rules

- For code, provide complete files or clear replacement blocks.
- For SQL, provide runnable queries or procedures.
- For documentation, use clean Markdown.
- For debugging, explain the root cause and the fix.
- For reports, clarify date ranges, sorting, and filters.

## Learning Rules

When a correction is likely to matter again:

- Add it to the related skill's `learnings.md`.
- If it applies globally, add it to `memory/recurring-corrections.md`.
- If it records a major direction, add it to `memory/decisions.md`.
