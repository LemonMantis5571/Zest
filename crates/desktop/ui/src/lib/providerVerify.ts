/** Session-scoped memory of the last gateway probe for a provider. */

export type VerifyMemory = {
  providerId: string;
  /** Unix ms when the last probe finished. */
  at: number;
  ok: boolean;
};

const memory = new Map<string, VerifyMemory>();

export function markProviderVerified(providerId: string) {
  memory.set(providerId, { providerId, at: Date.now(), ok: true });
}

export function markProviderVerifyFailed(providerId: string) {
  memory.set(providerId, { providerId, at: Date.now(), ok: false });
}

export function getProviderVerify(providerId: string): VerifyMemory | null {
  return memory.get(providerId) ?? null;
}

/** Recent failed probe — picker should not pretend Ready. */
export function recentVerifyFailed(providerId: string, maxAgeMs = 30 * 60 * 1000): boolean {
  const entry = memory.get(providerId);
  if (!entry || entry.ok) return false;
  return Date.now() - entry.at < maxAgeMs;
}
