import type { PreparedAttachment } from "./types.ts";

/** A user turn waiting behind the active turn for the same thread. */
export type QueuedTurn = {
  readonly id: string;
  readonly threadId: string;
  readonly text: string;
  readonly attachments: ReadonlyArray<PreparedAttachment>;
  readonly createdAt: number;
};

export type ThreadQueueMap = Record<string, ReadonlyArray<QueuedTurn>>;

export function enqueueThreadTurn(
  queues: ThreadQueueMap,
  threadId: string,
  turn: QueuedTurn
): ThreadQueueMap {
  return {
    ...queues,
    [threadId]: [...(queues[threadId] ?? []), turn],
  };
}

export function peekThreadTurn(
  queues: ThreadQueueMap,
  threadId: string
): QueuedTurn | undefined {
  return queues[threadId]?.[0];
}

export function removeThreadTurn(
  queues: ThreadQueueMap,
  threadId: string,
  turnId: string
): ThreadQueueMap {
  const current = queues[threadId];
  if (!current) return queues;

  const remaining = current.filter((turn) => turn.id !== turnId);
  if (remaining.length === current.length) return queues;

  const next = { ...queues };
  if (remaining.length > 0) {
    next[threadId] = remaining;
  } else {
    delete next[threadId];
  }
  return next;
}

export function updateThreadTurn(
  queues: ThreadQueueMap,
  threadId: string,
  turnId: string,
  text: string
): ThreadQueueMap {
  const current = queues[threadId];
  if (!current) return queues;

  let changed = false;
  const updated = current.map((turn) => {
    if (turn.id !== turnId) return turn;
    changed = true;
    return { ...turn, text };
  });
  return changed ? { ...queues, [threadId]: updated } : queues;
}

export function threadQueueCount(
  queues: ThreadQueueMap,
  threadId: string
): number {
  return queues[threadId]?.length ?? 0;
}
