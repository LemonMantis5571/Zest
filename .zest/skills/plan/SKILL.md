---
name: plan
description: Research the codebase and write an implementation plan without changing anything
---

Produce an implementation plan. Do not edit files, and do not run commands that
change anything — this turn ends with a plan, not with work done.

1. **Read first.** Find the code this actually touches. Name real files and real
   functions; a plan that says "update the relevant module" is not a plan.
2. **Reuse before adding.** Say which existing function or pattern each step
   builds on. Propose new code only where nothing suitable exists, and say why.
3. **Mark what suits another model, but do not hand it over yet.** If a step
   matches a routing task kind, name that kind in the step. Do not call
   `delegate` while planning: there is nothing to delegate until the plan
   exists, and a worker sees none of this conversation, so it would be guessing
   at work you have not written down. Build hands those pieces over.
4. **Write the plan** as ordered steps, each one small enough to verify. State
   how to check the whole thing works at the end: the command to run, the test
   to add, or what to look at in the app.
5. **Say what you are unsure about.** A step you are guessing at is worth more
   flagged than smoothed over.

Stop when the plan is written. Do not start building — "Build plan" under the
finished plan is how the user says go.
