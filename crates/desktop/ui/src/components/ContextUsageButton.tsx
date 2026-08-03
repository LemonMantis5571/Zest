import { useEffect, useId, useRef, useState } from "react";
import { XIcon } from "lucide-react";

import { getBackend } from "@/lib/backend";
import type { ContextUsage } from "@/lib/types";
import { cn } from "@/lib/utils";

function formatTokens(n: number) {
  if (n >= 1000) return `${(n / 1000).toFixed(n >= 10000 ? 0 : 1)}K`;
  return String(n);
}

type Props = {
  refreshKey: string | number;
  className?: string;
};

export function ContextUsageButton({ refreshKey, className }: Props) {
  const [usage, setUsage] = useState<ContextUsage | null>(null);
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const titleId = useId();

  useEffect(() => {
    let cancelled = false;
    getBackend()
      .contextUsage()
      .then((u) => {
        if (!cancelled) setUsage(u);
      })
      .catch(() => {
        if (!cancelled) setUsage(null);
      });
    return () => {
      cancelled = true;
    };
  }, [refreshKey]);

  useEffect(() => {
    if (!open) return;
    const onPointer = (e: PointerEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("pointerdown", onPointer);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", onPointer);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  if (!usage) {
    return (
      <span className={cn("text-[11px] text-muted-foreground", className)}>Context —</span>
    );
  }

  const pct = Math.round(usage.percentFull);
  const ring = Math.min(100, Math.max(0, usage.percentFull));

  return (
    <div ref={rootRef} className={cn("relative", className)}>
      <button
        type="button"
        title="Context usage"
        aria-expanded={open}
        aria-haspopup="dialog"
        onClick={() => setOpen((v) => !v)}
        className={cn(
          "inline-flex cursor-pointer items-center gap-1.5 rounded-md px-1.5 py-0.5 text-[11px] text-muted-foreground outline-none transition-colors",
          "hover:bg-secondary hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/50",
          open && "bg-secondary text-foreground"
        )}
      >
        <span
          className="relative grid size-3.5 place-items-center"
          aria-hidden
          style={{
            background: `conic-gradient(var(--primary) ${ring}%, color-mix(in srgb, var(--border) 80%, transparent) 0)`,
            borderRadius: "999px",
          }}
        >
          <span className="size-2 rounded-full bg-[var(--chat-canvas,#0c0c0e)]" />
        </span>
        <span>
          {pct}% · {formatTokens(usage.remainingTokens)} left
        </span>
      </button>

      {open ? (
        <div
          role="dialog"
          aria-labelledby={titleId}
          className="absolute bottom-[calc(100%+10px)] right-0 z-50 w-[280px] rounded-xl border border-border bg-popover p-3 text-popover-foreground shadow-xl"
        >
          <div className="mb-2 flex items-start justify-between gap-2">
            <div>
              <div id={titleId} className="text-sm font-semibold">
                Context usage
              </div>
              <div className="mt-0.5 text-[11px] text-muted-foreground">
                {pct}% full · ~{formatTokens(usage.usedTokens)} /{" "}
                {formatTokens(usage.windowTokens)}
              </div>
            </div>
            <button
              type="button"
              className="rounded-md p-1 text-muted-foreground hover:bg-secondary hover:text-foreground"
              onClick={() => setOpen(false)}
              aria-label="Close"
            >
              <XIcon className="size-3.5" />
            </button>
          </div>

          <p className="m-0 text-[10px] leading-snug text-muted-foreground">
            Source: {usage.source === "last_turn" ? "last API turn" : "estimate"}.
          </p>
        </div>
      ) : null}
    </div>
  );
}
