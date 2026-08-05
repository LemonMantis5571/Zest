import { FilePenLineIcon, TerminalIcon } from "lucide-react";

import type { ToolPart } from "@/lib/types";

type Props = {
  tools: ToolPart[];
  onFocusTool: (toolId: string) => void;
};

/**
 * Sticky status bar above the composer when approvals scrolled out of view.
 * The transcript card owns the approval actions; this strip only points back
 * to that card so there is one authoritative set of buttons.
 */
export function ApprovalStrip({ tools, onFocusTool }: Props) {
  if (tools.length === 0) return null;

  return (
    <div className="mb-2 space-y-1.5 rounded-xl border border-amber-500/30 bg-[color-mix(in_srgb,var(--card)_94%,transparent)] px-2.5 py-2 shadow-lg backdrop-blur-xl">
      <div className="px-1 text-[10px] font-medium uppercase tracking-wide text-amber-400/90">
        Needs your approval
      </div>
      {tools.map((tool) => {
        const isCommand = tool.name === "bash";
        return (
          <button
            type="button"
            key={tool.id}
            title="Show full approval card"
            className="flex w-full min-w-0 cursor-pointer items-center gap-2 rounded-lg bg-secondary/40 px-2 py-1.5 text-left outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
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
            <span className="ml-auto shrink-0 text-[10px] text-muted-foreground">
              Review above
            </span>
          </button>
        );
      })}
    </div>
  );
}
