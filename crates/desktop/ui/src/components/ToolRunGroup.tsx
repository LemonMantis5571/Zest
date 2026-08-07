import { useState } from "react";
import { ChevronRightIcon, XIcon } from "lucide-react";

import { ToolCallRow } from "@/components/ToolCallRow";
import type { ToolRunSummary } from "@/lib/toolRuns";
import type { ApprovalChoice, ToolPart } from "@/lib/types";
import { cn } from "@/lib/utils";

type Props = {
  tools: ToolPart[];
  summary: ToolRunSummary;
  onResolveApproval: (
    approvalId: string,
    decision: ApprovalChoice
  ) => Promise<void>;
  onOpenDiff?: (path: string, diff: string) => void;
  /** The one pending approval allowed to render a full card. */
  activeApprovalId?: string | null;
};

/**
 * One line standing in for a stretch of finished tool calls.
 *
 * Inspection-only runs collapse by default, but edit-containing runs stay open
 * so their diffs remain immediately reviewable. Failures are always stated on
 * the summary line because a fold that hides an error is worse than the rows.
 */
export function ToolRunGroup({
  tools,
  summary,
  onResolveApproval,
  onOpenDiff,
  activeApprovalId,
}: Props) {
  const hasChanges = summary.added > 0 || summary.removed > 0;
  // Completed edits contain the user's most important review surface. Keep
  // those cards visible; inspection-only runs can still collapse to one line.
  const [open, setOpen] = useState(hasChanges);

  if (open) {
    return (
      <div className="flex w-full max-w-full flex-col gap-0.5">
        <button
          type="button"
          onClick={() => setOpen(false)}
          className="flex w-full items-center gap-1.5 rounded-md px-1.5 py-1 text-left text-[12px] text-muted-foreground outline-none transition-colors hover:bg-white/[0.03] focus-visible:ring-2 focus-visible:ring-ring/40"
        >
          <ChevronRightIcon className="size-3 shrink-0 rotate-90 text-muted-foreground/50" />
          <span className="font-mono">{summary.label}</span>
          <span className="text-[11px] text-muted-foreground/60">— collapse</span>
        </button>
        <div className="flex flex-col gap-0.5 border-l border-border/40 pl-2">
          {tools.map((tool) => (
            <ToolCallRow
              key={tool.id}
              tool={tool}
              queued={
                tool.status === "awaiting_approval" && tool.id !== activeApprovalId
              }
              onResolveApproval={onResolveApproval}
              onOpenDiff={onOpenDiff}
            />
          ))}
        </div>
      </div>
    );
  }

  return (
    <button
      type="button"
      onClick={() => setOpen(true)}
      className={cn(
        "group/run flex w-full items-center gap-1.5 rounded-md px-1.5 py-1 text-left outline-none transition-colors",
        "hover:bg-white/[0.03] focus-visible:ring-2 focus-visible:ring-ring/40"
      )}
    >
      <span className="grid size-4 shrink-0 place-items-center">
        {summary.errors > 0 ? (
          <XIcon className="size-3 text-destructive" />
        ) : (
          <span className="size-1.5 rounded-full bg-muted-foreground/40" />
        )}
      </span>
      <span className="font-mono text-[12px] text-muted-foreground">
        {summary.label}
      </span>
      {hasChanges ? (
        <span className="shrink-0 font-mono text-[11px]">
          {summary.added > 0 ? (
            <span className="text-primary">+{summary.added}</span>
          ) : null}
          {summary.added > 0 && summary.removed > 0 ? " " : null}
          {summary.removed > 0 ? (
            <span className="text-destructive">-{summary.removed}</span>
          ) : null}
        </span>
      ) : null}
      {summary.errors > 0 ? (
        <span className="shrink-0 text-[11px] text-destructive">
          {summary.errors} failed
        </span>
      ) : null}
      <span className="min-w-0 flex-1" />
      <ChevronRightIcon className="size-3 shrink-0 text-muted-foreground/50" />
    </button>
  );
}
