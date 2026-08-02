import type { EffortId } from "./models.ts";
import { isEffortId } from "./models.ts";
import type { SessionInfo } from "./types.ts";

export type SessionOptionsSnapshot = {
  model: string;
  effort: EffortId;
};

/** Apply authoritative model/effort from Rust; keep local message list. */
export function mergeSessionOptions(
  prev: SessionInfo | null,
  info: SessionInfo
): SessionInfo {
  if (!prev) return info;
  return {
    ...info,
    messages: prev.messages,
  };
}

/** Roll back optimistic model/effort after a failed update. */
export function rollbackSessionOptions(
  session: SessionInfo | null,
  snapshot: SessionOptionsSnapshot
): SessionInfo | null {
  if (!session) return null;
  return {
    ...session,
    model: snapshot.model,
    effort: snapshot.effort,
  };
}

export function effortFromSession(effort: string, fallback: EffortId): EffortId {
  return isEffortId(effort) ? effort : fallback;
}
