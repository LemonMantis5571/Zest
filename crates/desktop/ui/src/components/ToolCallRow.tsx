import { useState } from "react";
import {
  CheckIcon,
  ChevronRightIcon,
  FilePenLineIcon,
  Maximize2Icon,
  TerminalIcon,
  XIcon,
} from "lucide-react";

import { DiffPreview } from "@/components/CodeBlock";
import { Button } from "@/components/ui/button";
import { ZestPulse } from "@/components/ZestPulse";
import type { ApprovalChoice, ToolPart } from "@/lib/types";
import { cn } from "@/lib/utils";

type Props = {
  tool: ToolPart;
  onResolveApproval: (
    approvalId: string,
    decision: ApprovalChoice
  ) => Promise<void>;
  onOpenDiff?: (path: string, diff: string) => void;
};

/**
 * Compact tool row — quiet chrome, expands for detail.
 * Rows with a diff open the full DiffViewer on click.
 */
export function ToolCallRow({ tool, onResolveApproval, onOpenDiff }: Props) {
  const awaiting = tool.status === "awaiting_approval";
  const [busy, setBusy] = useState<ApprovalChoice | null>(null);
  const [open, setOpen] = useState(false);
  const hasDiff = Boolean(tool.diff?.trim());

  async function resolve(decision: ApprovalChoice) {
    if (!tool.approvalId || busy !== null) return;
    setBusy(decision);
    try {
      await onResolveApproval(tool.approvalId, decision);
    } catch {
      setBusy(null);
    }
  }

  function openDiff() {
    if (!tool.diff?.trim() || !onOpenDiff) return;
    onOpenDiff(tool.path || tool.name, tool.diff);
  }

  // `bash` is the only tool that asks to run a command rather than change a
  // file; for it, `path` carries the command line verbatim.
  const isCommand = tool.name === "bash";

  if (awaiting) {
    return (
      <div
        data-tool-id={tool.id}
        className="w-full max-w-full overflow-hidden rounded-lg border border-border/50 bg-card/60"
      >
        <div className="flex items-start gap-2.5 px-3 py-2.5">
          <div className="mt-0.5 grid size-6 place-items-center rounded-md bg-muted/80 text-foreground">
            {isCommand ? (
              <TerminalIcon className="size-3.5" />
            ) : (
              <FilePenLineIcon className="size-3.5" />
            )}
          </div>
          <div className="min-w-0 flex-1">
            <div className="text-xs font-medium text-foreground">
              {isCommand ? "Run this command?" : `Allow ${tool.name}?`}
            </div>
            <TruncateWithHover
              text={
                tool.path ||
                tool.summary ||
                (isCommand ? "Run a command" : "Write to project file")
              }
              className="mt-0.5 font-mono text-[11px] text-muted-foreground"
            />
          </div>
          {hasDiff && onOpenDiff ? (
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              title="Open full diff"
              className="shrink-0"
              onClick={openDiff}
            >
              <Maximize2Icon className="size-3.5" />
            </Button>
          ) : null}
        </div>
        {tool.diff ? (
          <button
            type="button"
            title="Open full diff"
            className="block w-full cursor-pointer text-left outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/40"
            onClick={openDiff}
          >
            <DiffPreview diff={tool.diff} />
          </button>
        ) : null}
        <div className="flex flex-wrap items-center justify-end gap-2 px-3 py-2">
          <Button
            type="button"
            variant="ghost"
            size="sm"
            disabled={busy !== null}
            onClick={() => {
              void resolve("deny");
            }}
          >
            {busy === "deny" ? "Denying…" : "Deny"}
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            disabled={busy !== null}
            // The grant covers this tool and this exact target only, which is
            // what the row above is showing.
            title={
              isCommand
                ? "Stop asking about this exact command for the rest of the session"
                : `Stop asking about ${tool.path || "this file"} for the rest of the session`
            }
            onClick={() => {
              void resolve("session");
            }}
          >
            {busy === "session" ? "Allowing…" : "Allow for session"}
          </Button>
          <Button
            type="button"
            size="sm"
            disabled={busy !== null}
            onClick={() => {
              void resolve("once");
            }}
          >
            {busy === "once" ? (isCommand ? "Running…" : "Allowing…") : "Allow once"}
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
  const hasBody = Boolean(tool.summary?.trim()) || hasSkipped || hasDiff;

  const statusIcon =
    tool.status === "running" ? (
      <ZestPulse size={12} />
    ) : tool.status === "error" ? (
      <XIcon className="size-3 text-destructive" />
    ) : (
      <CheckIcon className="size-3 text-[var(--success,#27a644)]/90" />
    );

  return (
    <div
      className={cn(
        "group/tool w-full max-w-full rounded-lg",
        tool.status === "error" && "bg-destructive/5"
      )}
    >
      <button
        type="button"
        disabled={!hasBody}
        onClick={() => {
          if (hasDiff && onOpenDiff) {
            openDiff();
            return;
          }
          if (hasBody) setOpen((v) => !v);
        }}
        className={cn(
          "flex min-h-9 w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left outline-none transition-colors",
          "hover:bg-white/[0.035] focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/40",
          hasBody ? "cursor-pointer" : "cursor-default"
        )}
      >
        <span className="grid size-5 shrink-0 place-items-center rounded-md bg-muted/50">
          {statusIcon}
        </span>
        <span
          className={cn(
            "shrink-0 font-mono text-[12.5px] font-medium text-foreground/85",
            tool.status === "running" && "shimmer-text text-foreground/80"
          )}
        >
          {title}
        </span>
        {!delegation && (tool.path || tool.summary) ? (
          <TruncateWithHover
            text={tool.path || tool.summary || ""}
            className="min-w-0 flex-1 font-mono text-[11.5px] text-muted-foreground/75"
          />
        ) : (
          <span className="min-w-0 flex-1 text-[11.5px] text-muted-foreground/75">
            {hasSkipped ? `${skipped.length} fallback` : hasDiff ? "View diff" : null}
          </span>
        )}
        {hasDiff ? (
          <Maximize2Icon className="size-3 shrink-0 text-muted-foreground/50" />
        ) : hasBody ? (
          <ChevronRightIcon
            className={cn(
              "size-3 shrink-0 text-muted-foreground/50 transition-transform duration-150",
              open && "rotate-90"
            )}
          />
        ) : null}
      </button>
      {open && !hasDiff ? (
        <div className="mt-0.5 mb-1 space-y-1.5 px-2 pl-9">
          {hasSkipped ? (
            <div className="space-y-1">
              <div className="text-[10px] font-medium uppercase tracking-wide text-muted-foreground/80">
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
            <pre className="max-h-48 overflow-auto font-mono text-[11px] leading-relaxed text-muted-foreground/90 whitespace-pre-wrap">
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
