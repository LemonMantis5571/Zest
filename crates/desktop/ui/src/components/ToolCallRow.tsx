import { useState } from "react";
import {
  CheckIcon,
  ChevronRightIcon,
  FilePenLineIcon,
  XIcon,
} from "lucide-react";

import { DiffPreview } from "@/components/CodeBlock";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import type { ToolPart } from "@/lib/types";
import { cn } from "@/lib/utils";

type Props = {
  tool: ToolPart;
  onResolveApproval: (approvalId: string, allow: boolean) => Promise<void>;
};

/**
 * Compact tool row — Linear-style, not a heavy attachment card.
 * Truncated summaries show a portal-free hover preview (WebView-safe).
 */
export function ToolCallRow({ tool, onResolveApproval }: Props) {
  const awaiting = tool.status === "awaiting_approval";
  const [busy, setBusy] = useState<"allow" | "deny" | null>(null);
  const [open, setOpen] = useState(false);

  async function resolve(allow: boolean) {
    if (!tool.approvalId || busy !== null) return;
    setBusy(allow ? "allow" : "deny");
    try {
      await onResolveApproval(tool.approvalId, allow);
    } catch {
      setBusy(null);
    }
  }

  if (awaiting) {
    return (
      <div className="w-full max-w-full overflow-hidden rounded-lg border border-border/80 bg-card/90">
        <div className="flex items-start gap-2.5 border-b border-border/60 px-3 py-2.5">
          <div className="mt-0.5 grid size-7 place-items-center rounded-md bg-muted text-foreground">
            <FilePenLineIcon className="size-3.5" />
          </div>
          <div className="min-w-0 flex-1">
            <div className="text-xs font-medium text-foreground">
              Allow {tool.name}?
            </div>
            <TruncateWithHover
              text={tool.path || tool.summary || "Write to project file"}
              className="mt-0.5 font-mono text-[11px] text-muted-foreground"
            />
          </div>
        </div>
        {tool.diff ? (
          <DiffPreview diff={tool.diff} />
        ) : null}
        <div className="flex items-center justify-end gap-2 px-3 py-2">
          <Button
            type="button"
            variant="ghost"
            size="sm"
            disabled={busy !== null}
            onClick={() => {
              void resolve(false);
            }}
          >
            {busy === "deny" ? "Denying…" : "Deny"}
          </Button>
          <Button
            type="button"
            size="sm"
            disabled={busy !== null}
            onClick={() => {
              void resolve(true);
            }}
          >
            {busy === "allow" ? "Allowing…" : "Allow once"}
          </Button>
        </div>
      </div>
    );
  }

  const delegation =
    tool.metadata?.kind === "delegation" ? tool.metadata : null;
  const title = delegation
    ? `Delegated to ${delegation.provider_id} · ${delegation.model}`
    : tool.name;
  const skipped = delegation?.skipped ?? [];
  const hasSkipped = skipped.length > 0;
  const hasBody = Boolean(tool.summary?.trim()) || hasSkipped;

  const statusIcon =
    tool.status === "running" ? (
      <Spinner className="size-3.5" />
    ) : tool.status === "error" ? (
      <XIcon className="size-3.5 text-destructive" />
    ) : (
      <CheckIcon className="size-3.5 text-[var(--success,#27a644)]" />
    );

  return (
    <div
      className={cn(
        "group/tool w-full max-w-full rounded-lg border border-border/60 bg-card/40",
        tool.status === "error" && "border-destructive/40"
      )}
    >
      <button
        type="button"
        disabled={!hasBody}
        onClick={() => hasBody && setOpen((v) => !v)}
        className={cn(
          "flex w-full items-center gap-2 px-2.5 py-1.5 text-left outline-none",
          "hover:bg-accent/40 focus-visible:ring-2 focus-visible:ring-ring/40",
          !hasBody && "cursor-default"
        )}
      >
        <span className="grid size-5 shrink-0 place-items-center text-muted-foreground">
          {statusIcon}
        </span>
        <span
          className={cn(
            "shrink-0 font-mono text-[12px] font-medium text-foreground",
            tool.status === "running" && "shimmer-text"
          )}
        >
          {title}
        </span>
        {!delegation && tool.summary ? (
          <TruncateWithHover
            text={tool.summary}
            className="min-w-0 flex-1 font-mono text-[11px] text-muted-foreground"
          />
        ) : (
          <span className="min-w-0 flex-1 text-[11px] text-muted-foreground">
            {hasSkipped ? `${skipped.length} fallback` : null}
          </span>
        )}
        {hasBody ? (
          <ChevronRightIcon
            className={cn(
              "size-3.5 shrink-0 text-muted-foreground/70 transition-transform",
              open && "rotate-90"
            )}
          />
        ) : null}
      </button>
      {open ? (
        <div className="space-y-2 border-t border-border/50 bg-[var(--chat-canvas)] px-3 py-2">
          {hasSkipped ? (
            <div className="space-y-1">
              <div className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
                Fallback reasons
              </div>
              <ul className="space-y-0.5 text-[11px] text-muted-foreground">
                {skipped.map((s) => (
                  <li key={`${s.providerId}:${s.reason}`}>
                    skipped {s.providerId}: {s.reason}
                  </li>
                ))}
              </ul>
            </div>
          ) : null}
          {tool.summary ? (
            <pre className="max-h-56 overflow-auto font-mono text-[11px] leading-relaxed text-muted-foreground whitespace-pre-wrap">
              {tool.summary}
            </pre>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

/** Single-line truncate with in-tree hover card (no portal — WebView-safe). */
function TruncateWithHover({
  text,
  className,
}: {
  text: string;
  className?: string;
}) {
  return (
    <span className={cn("group/trunc relative min-w-0", className)}>
      <span className="block truncate">{text}</span>
      <span
        role="tooltip"
        className={cn(
          "pointer-events-none absolute bottom-[calc(100%+6px)] left-0 z-30 hidden w-max max-w-[min(22rem,70vw)]",
          "rounded-md border border-border/80 bg-popover px-2.5 py-1.5 text-left text-[11px] leading-snug text-popover-foreground shadow-lg",
          "whitespace-pre-wrap break-words",
          "group-hover/trunc:block"
        )}
      >
        {text}
      </span>
    </span>
  );
}
