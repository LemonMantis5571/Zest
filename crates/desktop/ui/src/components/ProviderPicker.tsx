import { BrandMark } from "@/components/BrandMark";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import type { ProviderRow } from "@/lib/types";

type Props = {
  providers: ProviderRow[];
  selectedId: string | null;
  error: string | null;
  onSelect: (id: string) => void;
  onContinue: () => void;
  onConnect: () => void;
  continuing: boolean;
};

export function ProviderPicker({
  providers,
  selectedId,
  error,
  onSelect,
  onContinue,
  onConnect,
  continuing,
}: Props) {
  const selected = providers.find((p) => p.id === selectedId) ?? null;
  const ready = selected?.statusKind === "ready" || selected?.statusKind === "unknown";

  return (
    <section className="w-full max-w-[420px]">
      <header className="mb-7">
        <div className="mb-4.5">
          <BrandMark />
        </div>
        <h1 className="m-0 mb-2 text-[28px] font-semibold leading-[1.2] tracking-[-0.6px]">
          Choose a provider
        </h1>
        <p className="m-0 max-w-[36ch] text-sm text-muted-foreground">
          Zest uses the sign-in your CLI already created. It never asks for your password.
        </p>
      </header>

      <div className="mb-2.5 text-xs font-medium text-muted-foreground">
        Available on this machine
      </div>

      <ul className="m-0 flex list-none flex-col gap-2 p-0" role="listbox" aria-label="Providers">
        {providers.map((p) => {
          const selectedRow = p.id === selectedId;
          const detail =
            p.statusKind === "ready"
              ? p.method
              : p.statusKind === "unknown"
                ? shortenUnknown(p.detail)
                : p.detail;

          return (
            <li key={p.id}>
              <button
                type="button"
                role="option"
                aria-selected={selectedRow}
                onClick={() => onSelect(p.id)}
                className={cn(
                  "grid w-full cursor-pointer grid-cols-[12px_1fr_auto] items-center gap-3 rounded-xl border bg-card px-3.5 py-3.5 text-left font-inherit transition-colors",
                  selectedRow
                    ? "border-primary shadow-[0_0_0_1px_color-mix(in_srgb,var(--primary)_35%,transparent)]"
                    : "border-border hover:border-[var(--hairline-strong,#34343a)] hover:bg-secondary"
                )}
              >
                <span
                  className={cn(
                    "justify-self-center size-2 rounded-full",
                    p.statusKind === "ready" && "bg-[var(--success)]",
                    p.statusKind === "unknown" && "bg-[#c4c4c4]",
                    (p.statusKind === "not_logged_in" || p.statusKind === "unconfigured") &&
                      "bg-transparent shadow-[inset_0_0_0_1.5px_var(--muted-foreground)]"
                  )}
                  aria-hidden
                />
                <span className="min-w-0">
                  <div className="text-[15px] font-semibold tracking-[-0.2px]">{p.label}</div>
                  <div className="mt-0.5 truncate text-xs text-muted-foreground">{detail}</div>
                </span>
                <span
                  className={cn(
                    "whitespace-nowrap text-xs font-medium text-muted-foreground",
                    p.statusKind === "ready" && "text-[var(--success)]",
                    p.statusKind === "unknown" && "text-[var(--ink-muted,#d0d6e0)]"
                  )}
                >
                  {p.statusLabel}
                </span>
              </button>
            </li>
          );
        })}
      </ul>

      {error ? <p className="mt-3.5 text-xs text-destructive">{error}</p> : null}

      <footer className="mt-7 flex justify-end gap-2.5">
        {selected?.canConnect ? (
          <Button type="button" variant="outline" onClick={onConnect}>
            {selected.statusKind === "ready" ? "Reconnect" : "Connect"}
          </Button>
        ) : null}
        <Button type="button" disabled={!ready || continuing} onClick={onContinue}>
          {continuing ? "Starting…" : "Continue"}
        </Button>
      </footer>
    </section>
  );
}

function shortenUnknown(detail: string) {
  if (detail.toLowerCase().includes("outside a readable file")) {
    return "Installed — session stored outside a readable file";
  }
  return detail;
}
