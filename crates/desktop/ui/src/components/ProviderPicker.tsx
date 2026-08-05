import { useState } from "react";
import { CheckIcon, FolderOpenIcon } from "lucide-react";

import { AuthShell } from "@/components/AuthShell";
import { BrandMark } from "@/components/BrandMark";
import { Button } from "@/components/ui/button";
import { recentVerifyFailed } from "@/lib/providerVerify";
import { getBackend } from "@/lib/backend";
import { cn } from "@/lib/utils";
import type { ProviderRow } from "@/lib/types";

type Props = {
  providers: ProviderRow[];
  selectedId: string | null;
  workspacePath: string | null;
  error: string | null;
  onSelect: (id: string) => void;
  onContinue: () => void;
  onConnect: () => void;
  onOpenFolder: () => void;
  onRefresh: () => Promise<void>;
  continuing: boolean;
};

function shortRoot(root: string): string {
  const cleaned = root.replace(/^\\\\\?\\UNC\\/i, "\\\\").replace(/^\\\\\?\\/, "");
  const normalized = cleaned.replace(/\\/g, "/");
  const parts = normalized.split("/").filter(Boolean);
  if (parts.length <= 2) return cleaned;
  return parts.slice(-2).join("/");
}

export function ProviderPicker({
  providers,
  selectedId,
  workspacePath,
  error,
  onSelect,
  onContinue,
  onConnect,
  onOpenFolder,
  onRefresh,
  continuing,
}: Props) {
  const [apiKey, setApiKey] = useState("");
  const [savingKey, setSavingKey] = useState(false);
  const [keyError, setKeyError] = useState<string | null>(null);
  const selected = providers.find((p) => p.id === selectedId) ?? null;
  const selectedNeedsConnect =
    selected != null && recentVerifyFailed(selected.id);
  const ready =
    !selectedNeedsConnect &&
    (selected?.statusKind === "ready" || selected?.statusKind === "unknown");

  return (
    <AuthShell>
      <header className="mb-6">
        <div className="mb-4">
          <BrandMark />
        </div>
        <h1 className="m-0 mb-1.5 text-[22px] font-semibold leading-tight tracking-[-0.4px]">
          Choose a provider
        </h1>
        <p className="m-0 max-w-[38ch] text-[13px] leading-relaxed text-muted-foreground">
          Uses the sign-in your CLI already created — Zest never asks for your password.
        </p>
      </header>

      <div className="mb-5 flex items-center gap-2 border-b border-border/60 pb-4">
        <div className="min-w-0 flex-1">
          <div className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
            Project folder
          </div>
          <div
            className="mt-0.5 truncate font-mono text-xs text-foreground/85"
            title={workspacePath ?? undefined}
          >
            {workspacePath ? shortRoot(workspacePath) : "Current directory"}
          </div>
        </div>
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={continuing}
          onClick={onOpenFolder}
        >
          <FolderOpenIcon className="size-3.5" />
          Open
        </Button>
      </div>

      <div className="mb-2 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
        Available on this machine
      </div>

      <ul
        className="m-0 list-none overflow-hidden rounded-lg border border-border/70 bg-card/40 p-0"
        role="listbox"
        aria-label="Providers"
      >
        {providers.map((p, index) => {
          const selectedRow = p.id === selectedId;
          const verifyFailed = recentVerifyFailed(p.id);
          const detail = verifyFailed
            ? "Last check failed — Connect again"
            : p.statusKind === "ready"
              ? p.method
              : p.statusKind === "unknown"
                ? shortenUnknown(p.detail)
                : p.detail;
          const statusLabel = verifyFailed ? "Reconnect" : p.statusLabel;

          return (
            <li
              key={p.id}
              className="animate-in fade-in slide-in-from-bottom-1 fill-mode-both duration-200"
              style={{ animationDelay: `${40 + index * 35}ms` }}
            >
              <button
                type="button"
                role="option"
                aria-selected={selectedRow}
                onClick={() => onSelect(p.id)}
                className={cn(
                  "grid w-full cursor-pointer grid-cols-[10px_1fr_auto] items-center gap-3 px-3.5 py-3 text-left font-inherit outline-none transition-[background-color,color] duration-150",
                  "border-b border-border/50 last:border-b-0",
                  "hover:bg-accent/50 focus-visible:bg-accent/50 focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/40",
                  selectedRow && "bg-accent/70"
                )}
              >
                <span
                  className={cn(
                    "justify-self-center size-2 rounded-full transition-colors duration-150",
                    !verifyFailed && p.statusKind === "ready" && "bg-[var(--success)]",
                    verifyFailed && "bg-amber-400",
                    !verifyFailed && p.statusKind === "unknown" && "bg-[#c4c4c4]",
                    !verifyFailed &&
                      (p.statusKind === "not_logged_in" ||
                        p.statusKind === "unconfigured") &&
                      "bg-transparent shadow-[inset_0_0_0_1.5px_var(--muted-foreground)]"
                  )}
                  aria-hidden
                />
                <span className="min-w-0">
                  <div className="flex items-center gap-2 text-[13px] font-medium tracking-[-0.1px]">
                    {p.label}
                    {selectedRow ? (
                      <CheckIcon
                        className="size-3 text-primary"
                        strokeWidth={2.5}
                        aria-hidden
                      />
                    ) : null}
                  </div>
                  <div className="mt-0.5 truncate text-[11px] text-muted-foreground">{detail}</div>
                </span>
                <span
                  className={cn(
                    "whitespace-nowrap text-[11px] font-medium text-muted-foreground",
                    !verifyFailed && p.statusKind === "ready" && "text-[var(--success)]",
                    verifyFailed && "text-amber-400"
                  )}
                >
                  {statusLabel}
                </span>
              </button>
            </li>
          );
        })}
      </ul>

      {error ? <p className="mt-3 text-xs text-destructive">{error}</p> : null}

      {selected?.method === "API key" ? (
        <div className="mt-4 rounded-lg border border-border/70 bg-card/40 p-3">
          <div className="text-xs font-medium">{selected.label} API key</div>
          <p className="mt-1 text-[11px] leading-relaxed text-muted-foreground">
            Stored securely in the operating system credential manager.
          </p>
          <div className="mt-2 flex gap-1.5">
            <input
              type="password"
              value={apiKey}
              onChange={(event) => setApiKey(event.target.value)}
              placeholder={selected.statusKind === "ready" ? "Replace key" : "Paste API key"}
              autoComplete="off"
              className="min-w-0 flex-1 rounded-md border border-border/80 bg-background px-2.5 py-1.5 text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
            />
            <Button
              type="button"
              size="sm"
              disabled={!apiKey.trim() || savingKey}
              onClick={() => {
                setSavingKey(true);
                setKeyError(null);
                void getBackend()
                  .setProviderKey(selected.id, apiKey)
                  .then(() => {
                    setApiKey("");
                    return onRefresh();
                  })
                  .catch((err) => setKeyError(String(err)))
                  .finally(() => setSavingKey(false));
              }}
            >
              {savingKey ? "Saving…" : "Save"}
            </Button>
          </div>
          {keyError ? <p className="mt-2 text-xs text-destructive">{keyError}</p> : null}
        </div>
      ) : null}

      <footer className="mt-6 flex justify-end gap-2">
        {selected?.canConnect ? (
          <Button type="button" variant="outline" onClick={onConnect}>
            {selected.statusKind === "ready" || selectedNeedsConnect
              ? "Reconnect"
              : "Connect"}
          </Button>
        ) : null}
        <Button type="button" disabled={!ready || continuing} onClick={onContinue}>
          {continuing ? "Starting…" : "Continue"}
        </Button>
      </footer>
    </AuthShell>
  );
}

function shortenUnknown(detail: string) {
  if (detail.toLowerCase().includes("outside a readable file")) {
    return "Installed — session stored outside a readable file";
  }
  return detail;
}
