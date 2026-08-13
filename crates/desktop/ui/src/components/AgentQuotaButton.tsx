import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { GaugeIcon, RefreshCwIcon } from "lucide-react";

import { TopbarPanel } from "@/components/TopbarPanel";
import { Button } from "@/components/ui/button";
import { getBackend } from "@/lib/backend";
import { createProviderQuotaLoader } from "@/lib/quotaCache";
import type { ProviderQuotaSnapshot, ProviderRow, UsageSnapshot } from "@/lib/types";

type Props = {
  providers: ProviderRow[];
  refreshKey: string | number;
};

export function AgentQuotaButton({ providers, refreshKey }: Props) {
  const [snapshot, setSnapshot] = useState<UsageSnapshot | null>(null);
  const [liveQuota, setLiveQuota] = useState<ProviderQuotaSnapshot | null>(null);
  const [quotaLoading, setQuotaLoading] = useState(false);
  const [quotaError, setQuotaError] = useState<string | null>(null);
  const quotaRequestRef = useRef(0);
  const quotaLoader = useMemo(
    () => createProviderQuotaLoader(() => getBackend().providerQuota()),
    []
  );

  useEffect(() => {
    let live = true;
    const backend = getBackend();
    void backend
      .usageSnapshot()
      .then((next) => {
        if (live) setSnapshot(next);
      })
      .catch(() => {
        if (live) setSnapshot(null);
      });
    return () => {
      live = false;
    };
  }, [refreshKey]);

  const loadQuota = useCallback((force = false) => {
    const requestId = quotaRequestRef.current + 1;
    quotaRequestRef.current = requestId;
    setQuotaLoading(true);
    setQuotaError(null);

    return quotaLoader
      .load(force)
      .then((result) => {
        if (requestId !== quotaRequestRef.current || result.kind === "stale") return;
        if (result.kind === "error") {
          setQuotaError(
            result.snapshot
              ? "Could not refresh provider limits. Showing the last result."
              : "Could not check provider limits."
          );
          return;
        }
        setLiveQuota(result.snapshot);
      })
      .finally(() => {
        if (requestId !== quotaRequestRef.current) return;
        setQuotaLoading(false);
      });
  }, [quotaLoader]);

  const rows = useMemo(() => {
    const ids = providers.length
      ? providers.map((provider) => provider.id)
      : (snapshot?.providers ?? []).map((provider) => provider.providerId);
    return Array.from(new Set(ids)).map((id) => ({
      id,
      label: providers.find((provider) => provider.id === id)?.label ?? id,
      headroom: snapshot?.providers.find((provider) => provider.providerId === id)?.headroom,
      quota: liveQuota?.providers.find((provider) => provider.providerId === id),
    }));
  }, [liveQuota, providers, snapshot]);

  return (
    <TopbarPanel
      icon={GaugeIcon}
      label="Agent quota"
      onOpenChange={(open) => {
        if (open) void loadQuota();
      }}
    >
      <div className="flex flex-col gap-2.5">
        <div className="flex items-start justify-between gap-3">
          <div>
            <h2 className="m-0 text-sm font-semibold">Agent quota</h2>
            <p className="m-0 mt-0.5 text-[11px] text-muted-foreground">
              Real balance or limits from the provider.
            </p>
          </div>
          <Button
            type="button"
            variant="outline"
            size="icon-xs"
            title="Refresh quota"
            aria-label="Refresh quota"
            disabled={quotaLoading}
            onClick={() => void loadQuota(true)}
          >
            <RefreshCwIcon
              className={quotaLoading ? "animate-spin" : undefined}
              aria-hidden="true"
            />
          </Button>
        </div>

        {quotaLoading ? (
          <p className="m-0 text-[11px] text-muted-foreground">
            {liveQuota ? "Updating provider limits…" : "Checking provider limits…"}
          </p>
        ) : null}
        {quotaError ? (
          <p role="status" className="m-0 text-[11px] text-amber-300">
            {quotaError}
          </p>
        ) : null}

        {rows.length ? (
          <div className="flex flex-col gap-1.5">
            {rows.map((row) => (
              <QuotaRow
                key={row.id}
                label={row.label}
                headroom={row.headroom}
                quota={row.quota}
              />
            ))}
          </div>
        ) : (
          <p className="m-0 rounded-md border border-dashed border-border/70 px-2.5 py-2 text-[11px] text-muted-foreground">
            No providers are configured yet.
          </p>
        )}

        <p className="m-0 border-t border-border/60 pt-2 text-[10px] leading-relaxed text-muted-foreground">
          Zest shows only values returned by the provider. If a provider has no supported quota
          check, that is shown instead of a guessed number.
        </p>
      </div>
    </TopbarPanel>
  );
}

function QuotaRow({
  label,
  headroom,
  quota,
}: {
  label: string;
  headroom: UsageSnapshot["providers"][number]["headroom"] | undefined;
  quota: ProviderQuotaSnapshot["providers"][number] | undefined;
}) {
  const reported = headroom?.kind === "provider_reported" ? headroom : null;
  const balance = quota?.kind === "balance" ? quota : null;
  const rateLimit = quota?.kind === "rate_limit" ? quota : null;
  const requestLine = reported?.requestsRemaining != null
    ? `${reported.requestsRemaining.toLocaleString()} requests left${
        reported.requestsLimit != null ? ` of ${reported.requestsLimit.toLocaleString()}` : ""
      }`
    : null;
  const tokenLine = reported
    ? [
        reported.inputTokensRemaining != null
          ? `${reported.inputTokensRemaining.toLocaleString()} input`
          : null,
        reported.outputTokensRemaining != null
          ? `${reported.outputTokensRemaining.toLocaleString()} output`
          : reported.tokensRemaining != null
            ? `${reported.tokensRemaining.toLocaleString()} tokens`
            : null,
      ]
        .filter(Boolean)
        .join(" · ")
    : "";
  const reset = reported?.requestsReset ?? reported?.tokensReset;

  return (
    <div className="rounded-md border border-border/70 bg-secondary/30 px-2.5 py-2">
      <div className="flex items-baseline justify-between gap-2">
        <span className="truncate text-xs font-medium" title={label}>
          {label}
        </span>
        {reported?.ageSecs != null ? (
          <span className="shrink-0 text-[10px] text-muted-foreground">
            {formatAge(reported.ageSecs)} ago
          </span>
        ) : null}
      </div>
      {balance ? (
        <div className="mt-1 space-y-0.5 text-[10px] text-muted-foreground">
          {balance.balances.length ? (
            balance.balances.map((entry) => (
              <div key={entry.currency} className="text-foreground/85">
                {entry.totalBalance} {entry.currency}{" "}
                {balance.available === false ? "reported" : "available"}
              </div>
            ))
          ) : (
            <div className="text-foreground/85">No balance details returned.</div>
          )}
          <div>{balance.detail}</div>
        </div>
      ) : rateLimit ? (
        <div className="mt-1 space-y-0.5 text-[10px] text-muted-foreground">
          {rateLimit.plan ? (
            <div className="text-foreground/85">Plan: {formatPlan(rateLimit.plan)}</div>
          ) : null}
          {rateLimit.windows.map((quotaWindow, index) => (
            <div key={quotaWindow.label + "-" + index}>
              <span className="text-foreground/85">
                {quotaWindow.label}: {formatPercentLeft(quotaWindow.usedPercent)} left
              </span>
              {quotaWindow.resetsAt != null
                ? " · resets " + formatResetEpoch(quotaWindow.resetsAt)
                : ""}
            </div>
          ))}
          {rateLimit.spendLimit ? (
            <div>
              <span className="text-foreground/85">
                Monthly: {formatPercent(rateLimit.spendLimit.remainingPercent)} left
              </span>
              {rateLimit.spendLimit.resetsAt != null
                ? " · resets " + formatResetEpoch(rateLimit.spendLimit.resetsAt)
                : ""}
            </div>
          ) : null}
          <div>{rateLimit.detail}</div>
        </div>
      ) : reported ? (
        <div className="mt-1 space-y-0.5 text-[10px] text-muted-foreground">
          {reported.quotaWindow ? (
            <div className="text-foreground/85">
              {formatQuotaWindow(reported.quotaWindow)}
              {reported.quotaUsedPercent != null
                ? ": " + formatPercent(reported.quotaUsedPercent) + " used"
                : reported.quotaStatus
                  ? ": " + formatQuotaStatus(reported.quotaStatus)
                  : ""}
            </div>
          ) : null}
          {reported.quotaResetAt != null ? (
            <div>Resets: {formatResetEpoch(reported.quotaResetAt)}</div>
          ) : null}
          {reported.quotaOverageStatus ? (
            <div>
              Extra use: {formatQuotaStatus(reported.quotaOverageStatus)}
              {reported.quotaOverageResetAt != null
                ? " · resets " + formatResetEpoch(reported.quotaOverageResetAt)
                : ""}
            </div>
          ) : null}
          {reported.quotaIsUsingOverage ? <div>Using extra capacity</div> : null}
          {requestLine ? (
            <div className="text-foreground/85">{requestLine}</div>
          ) : !reported.quotaWindow ? (
            <div className="text-foreground/85">Requests shared by provider</div>
          ) : null}
          {tokenLine ? <div>Tokens: {tokenLine}</div> : null}
          {reset ? <div>Reset: {formatReset(reset)}</div> : null}
          {reported.retryAfterSecs != null ? (
            <div className="text-amber-300">Try again in {formatRetry(reported.retryAfterSecs)}</div>
          ) : null}
        </div>
      ) : (
        <div className="mt-0.5 text-[10px] text-muted-foreground">
          {quota?.detail ?? headroom?.label ?? "No quota data returned."}
        </div>
      )}
    </div>
  );
}

function formatAge(secs: number): string {
  if (secs < 60) return `${Math.max(1, secs)}s`;
  if (secs < 3600) return `${Math.floor(secs / 60)}m`;
  if (secs < 86_400) return `${Math.floor(secs / 3600)}h`;
  return `${Math.floor(secs / 86_400)}d`;
}

function formatRetry(secs: number): string {
  if (secs < 60) return `${Math.max(1, secs)}s`;
  if (secs < 3600) return `${Math.ceil(secs / 60)}m`;
  return `${Math.ceil(secs / 3600)}h`;
}

function formatReset(value: string): string {
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) return value;
  return parsed.toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}

function formatResetEpoch(value: number): string {
  return formatReset(new Date(value * 1000).toISOString());
}

function formatPercentLeft(usedPercent: number): string {
  return formatPercent(Math.max(0, 100 - usedPercent));
}

function formatPercent(value: number): string {
  return String(Math.max(0, Math.min(100, value)).toFixed(0)) + "%";
}

function formatPlan(value: string): string {
  return value
    .replace(/[_-]+/g, " ")
    .replace(/\b\w/g, (letter) => letter.toUpperCase());
}

function formatQuotaWindow(value: string): string {
  return formatPlan(value.replace(/_/g, " "));
}

function formatQuotaStatus(value: string): string {
  return formatPlan(value.replace(/_/g, " "));
}
