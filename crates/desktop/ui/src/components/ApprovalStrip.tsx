import { useState } from "react";
import { FilePenLineIcon, TerminalIcon } from "lucide-react";

import { Button } from "@/components/ui/button";
import type { ApprovalChoice, ToolPart } from "@/lib/types";

type Props = {
  tools: ToolPart[];
  onResolveApproval: (
    approvalId: string,
    decision: ApprovalChoice
  ) => Promise<void>;
  onFocusTool: (toolId: string) => void;
};

/**
 * Sticky bar above the composer when approvals scrolled out of view.
 * Actions match ToolCallRow; strip click scrolls to the full card.
 */
export function ApprovalStrip({
  tools,
  onResolveApproval,
  onFocusTool,
}: Props) {
  const [busy, setBusy] = useState<{
    approvalId: string;
    decision: ApprovalChoice;
  } | null>(null);

  if (tools.length === 0) return null;

  async function resolve(approvalId: string, decision: ApprovalChoice) {
    if (busy !== null) return;
    setBusy({ approvalId, decision });
    try {
      await onResolveApproval(approvalId, decision);
    } catch {
      setBusy(null);
    }
  }

  return (
    <div className="mb-2 space-y-1.5 rounded-xl border border-amber-500/30 bg-[color-mix(in_srgb,var(--card)_94%,transparent)] px-2.5 py-2 shadow-lg backdrop-blur-xl">
      <div className="px-1 text-[10px] font-medium uppercase tracking-wide text-amber-400/90">
        Needs your approval
      </div>
      {tools.map((tool) => {
        const isCommand = tool.name === "bash";
        const approvalId = tool.approvalId;
        if (!approvalId) return null;
        const rowBusy = busy?.approvalId === approvalId ? busy.decision : null;
        return (
          <div
            key={tool.id}
            className="flex flex-wrap items-center gap-2 rounded-lg bg-secondary/40 px-2 py-1.5"
          >
            <button
              type="button"
              title="Show full approval card"
              className="flex min-w-0 flex-1 cursor-pointer items-center gap-2 text-left outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
              onClick={() => onFocusTool(tool.id)}
            >
              <span className="grid size-6 shrink-0 place-items-center rounded-md bg-muted/80">
                {isCommand ? (
                  <TerminalIcon className="size-3.5" />
                ) : (
                  <FilePenLineIcon className="size-3.5" />
                )}
              </span>
              <span className="min-w-0">
                <span className="block truncate text-xs font-medium text-foreground">
                  {isCommand ? "Run this command?" : `Allow ${tool.name}?`}
                </span>
                <span className="block truncate font-mono text-[10px] text-muted-foreground">
                  {tool.path || tool.summary || tool.name}
                </span>
              </span>
            </button>
            <div className="flex shrink-0 flex-wrap items-center justify-end gap-1">
              <Button
                type="button"
                variant="ghost"
                size="sm"
                disabled={busy !== null}
                onClick={() => {
                  void resolve(approvalId, "deny");
                }}
              >
                {rowBusy === "deny" ? "Denying…" : "Deny"}
              </Button>
              <Button
                type="button"
                variant="ghost"
                size="sm"
                disabled={busy !== null}
                title="Allow for session"
                onClick={() => {
                  void resolve(approvalId, "session");
                }}
              >
                {rowBusy === "session" ? "Allowing…" : "Session"}
              </Button>
              <Button
                type="button"
                size="sm"
                disabled={busy !== null}
                onClick={() => {
                  void resolve(approvalId, "once");
                }}
              >
                {rowBusy === "once" ? "…" : "Allow"}
              </Button>
            </div>
          </div>
        );
      })}
    </div>
  );
}
