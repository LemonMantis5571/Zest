import type { MeasuredUsage } from "./types.ts";

type CacheProviderUsage = {
  measured: Pick<MeasuredUsage, "inputTokens" | "cacheReadTokens" | "cacheWriteTokens">;
};

export type CacheMetrics = {
  cachedInputTokens: number;
  cacheWriteTokens: number;
  promptTokens: number;
  hitPercent: number;
};

/**
 * Summarise provider-reported prompt caching: input + cache reads + cache writes
 * is the full prompt volume, and cache reads are the portion served from the
 * provider cache.
 */
export function cacheMetrics(
  providers: ReadonlyArray<CacheProviderUsage>
): CacheMetrics | null {
  const totals = providers.reduce(
    (sum, provider) => ({
      inputTokens: sum.inputTokens + provider.measured.inputTokens,
      cachedInputTokens: sum.cachedInputTokens + provider.measured.cacheReadTokens,
      cacheWriteTokens: sum.cacheWriteTokens + provider.measured.cacheWriteTokens,
    }),
    { inputTokens: 0, cachedInputTokens: 0, cacheWriteTokens: 0 }
  );
  const promptTokens =
    totals.inputTokens + totals.cachedInputTokens + totals.cacheWriteTokens;

  if (promptTokens <= 0) return null;

  return {
    cachedInputTokens: totals.cachedInputTokens,
    cacheWriteTokens: totals.cacheWriteTokens,
    promptTokens,
    hitPercent: (totals.cachedInputTokens / promptTokens) * 100,
  };
}
