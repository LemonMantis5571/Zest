import { useEffect } from "react";
import { XIcon } from "lucide-react";

import { Button } from "@/components/ui/button";
import { recentVerifyFailed } from "@/lib/providerVerify";
import type { ProviderRow } from "@/lib/types";
import { cn } from "@/lib/utils";

type Props = {
  open: boolean;
  providers: ProviderRow[];
  currentProviderId: string;
  busy: boolean;
  onClose: () => void;
  onSelect: (providerId: string) => void;
  onConnect: (providerId: string) => void;
};

function statusLabel(row: ProviderRow): string {
  if (row.statusKind === "ready" && recentVerifyFailed(row.id)) {
    return "Needs Connect again";
  }
  if (row.statusKind === "ready") return row.method || "Ready";
  if (row.statusKind === "unknown") return row.detail || "Unknown";
  return row.detail || "Not ready";
}

/**
 * In-chat provider switch — WebView-safe positioned panel (no portals).
 */
export function ProviderSwitchSheet({
  open,
  providers,
  currentProviderId,
  busy,
  onClose,
  onSelect,
  onConnect,
}: Props) {
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !busy) onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [busy, onClose, open]);

  if (!open) return null;

  return (
    <div className="absolute inset-0 z-40 flex items-end justify-center bg-black/45 p-4 sm:items-center">
      <button
        type="button"
        aria-label="Dismiss"
        className="absolute inset-0 cursor-default"
        disabled={busy}
        onClick={() => {
          if (!busy) onClose();
        }}
      />
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Change provider"
        className="relative z-10 w-full max-w-md overflow-hidden rounded-xl border border-border bg-popover text-popover-foreground shadow-2xl"
      >
        <div className="flex items-center justify-between border-b border-border/60 px-4 py-3">
          <div>
            <div className="text-sm font-semibold">Change provider</div>
            <div className="text-[11px] text-muted-foreground">
              Switch without leaving this chat shell
            </div>
          </div>
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            title="Close"
            disabled={busy}
            onClick={onClose}
          >
            <XIcon />
          </Button>
        </div>
        <ul className="m-0 max-h-[50vh] list-none overflow-y-auto p-2">
          {providers.map((row) => {
            const current = row.id === currentProviderId;
            const failed = recentVerifyFailed(row.id);
            const selectable =
              (row.statusKind === "ready" || row.statusKind === "unknown") &&
              !failed;
            return (
              <li key={row.id}>
                <div
                  className={cn(
                    "flex items-center gap-2 rounded-lg px-2.5 py-2",
                    current && "bg-secondary/60"
                  )}
                >
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-sm font-medium">{row.label}</div>
                    <div className="truncate text-[11px] text-muted-foreground">
                      {statusLabel(row)}
                    </div>
                  </div>
                  {current ? (
                    <span className="shrink-0 text-[11px] text-muted-foreground">
                      Current
                    </span>
                  ) : selectable ? (
                    <Button
                      type="button"
                      size="sm"
                      disabled={busy}
                      onClick={() => onSelect(row.id)}
                    >
                      Switch
                    </Button>
                  ) : (
                    <Button
                      type="button"
                      size="sm"
                      variant="outline"
                      disabled={busy}
                      onClick={() => onConnect(row.id)}
                    >
                      Connect
                    </Button>
                  )}
                </div>
              </li>
            );
          })}
        </ul>
      </div>
    </div>
  );
}
