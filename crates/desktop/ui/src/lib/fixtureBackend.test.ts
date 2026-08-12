import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { createFixtureBackend } from "./fixtureBackend.ts";
import type { DelegationEvent } from "./types.ts";

describe("fixture chat rename", () => {
  it("persists a trimmed title through the sidebar listing", async () => {
    const backend = createFixtureBackend();

    const [thread] = await backend.listThreads();
    const renamed = await backend.renameThread(thread.id, ".", "  Release checklist  ");

    assert.equal(renamed.title, "Release checklist");
    assert.equal(
      (await backend.listThreads()).find((item) => item.id === thread.id)?.title,
      "Release checklist"
    );
  });

  it("rejects an empty title", async () => {
    const backend = createFixtureBackend();
    const [thread] = await backend.listThreads();

    await assert.rejects(
      backend.renameThread(thread.id, ".", "   "),
      /chat title is empty/
    );
  });
});

describe("fixture delegation lifecycle", () => {
  it("drives a card through worker, reviewer, ready, and apply states", async () => {
    const backend = createFixtureBackend();
    const events: DelegationEvent[] = [];
    const unlisten = await backend.onDelegationEvent((event) => events.push(event));

    const [initial] = await backend.listDelegationJobs();
    assert.equal(initial.status, "awaiting_approval");
    assert.equal(initial.changedFileCount, 0);

    const ready = await backend.retryDelegationJob(initial.jobId);
    assert.equal(ready.status, "ready_to_apply");
    assert.deepEqual(
      events.map((event) => event.kind),
      [
        "worker_started",
        "worker_completed",
        "reviewer_started",
        "reviewer_completed",
        "ready_to_apply",
      ]
    );
    assert.deepEqual(ready.changedFiles, ["src/fixture.ts"]);
    assert.equal(ready.acceptanceChecks[0]?.status, "passed");

    const applied = await backend.applyDelegationJob(initial.jobId);
    assert.equal(applied.status, "accepted");
    assert.equal(events.at(-1)?.kind, "applied");
    assert.equal((await backend.getDelegationJob(initial.jobId)).status, "accepted");

    unlisten();
  });

  it("makes cancellation observable from the fixture board", async () => {
    const backend = createFixtureBackend();
    const events: DelegationEvent[] = [];
    await backend.onDelegationEvent((event) => events.push(event));
    const [job] = await backend.listDelegationJobs();

    const cancelled = await backend.cancelDelegationJob(job.jobId);
    assert.equal(cancelled.status, "cancelled");
    assert.equal(events.at(-1)?.kind, "cancelled");
  });
});
