import { useState, type ReactNode } from "react";
import {
  CheckIcon,
  ChevronDownIcon,
  CopyIcon,
  DownloadIcon,
  LightbulbIcon,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

type Props = {
  /** Slash command that produced this answer, e.g. `plan`. */
  command: string;
  /** Raw markdown, for copy and download. */
  text: string;
  /** Rendered body. */
  children: ReactNode;
  /** Still streaming — hide the actions until there is something to act on. */
  streaming?: boolean;
  /**
   * What to do with this document, offered under it.
   *
   * Stays generic on purpose: the card knows a command produced a document, not
   * that plans get built. Whoever renders the card decides what follows one.
   */
  action?: {
    label: string;
    hint?: string;
    onClick: () => void;
    disabled?: boolean;
  };
};

/**
 * Frames the answer to a slash command as a document rather than a chat reply.
 *
 * Deliberately generic: the title is the command name, so a new `.zest/skills`
 * entry gets the same treatment with no code change. That mirrors the rule that
 * commands are markdown files, not Rust.
 */
export function CommandOutputCard({
  command,
  text,
  children,
  streaming,
  action,
}: Props) {
  const [collapsed, setCollapsed] = useState(false);
  const [copied, setCopied] = useState(false);

  const title = command.charAt(0).toUpperCase() + command.slice(1);

  async function copy() {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1200);
    } catch {
      /* clipboard denied — the text is still selectable */
    }
  }

  function download() {
    // Blob + object URL keeps this self-contained; no backend round-trip for
    // something the UI already holds in full.
    const blob = new Blob([text], { type: "text/markdown;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `${command}.md`;
    a.click();
    URL.revokeObjectURL(url);
  }

  return (
    <div className="w-full max-w-full overflow-hidden rounded-xl border border-border/70 bg-card/50">
      <div className="flex items-center gap-2 border-b border-border/50 px-3 py-2">
        <LightbulbIcon className="size-3.5 shrink-0 text-muted-foreground" />
        <span className="min-w-0 flex-1 truncate text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
          {title}
        </span>

        {!streaming ? (
          <div className="flex shrink-0 items-center gap-0.5">
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              title="Copy markdown"
              onClick={() => void copy()}
            >
              {copied ? (
                <CheckIcon className="size-3.5 text-[var(--success,#27a644)]" />
              ) : (
                <CopyIcon className="size-3.5" />
              )}
            </Button>
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              title={`Save as ${command}.md`}
              onClick={download}
            >
              <DownloadIcon className="size-3.5" />
            </Button>
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              title={collapsed ? "Expand" : "Collapse"}
              aria-expanded={!collapsed}
              onClick={() => setCollapsed((v) => !v)}
            >
              <ChevronDownIcon
                className={cn(
                  "size-3.5 transition-transform duration-150",
                  collapsed && "-rotate-90"
                )}
              />
            </Button>
          </div>
        ) : null}
      </div>

      {collapsed ? (
        <button
          type="button"
          onClick={() => setCollapsed(false)}
          className="w-full px-3 py-2 text-left text-[11px] text-muted-foreground outline-none hover:bg-white/[0.03] focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/40"
        >
          {/* Length is the only honest summary available without parsing. */}
          {text.split("\n").length} lines — click to expand
        </button>
      ) : (
        <div className="px-3 py-2.5">{children}</div>
      )}

      {/* Hidden while streaming and while collapsed: acting on a document you
          cannot see, or that is not finished, is not a choice worth offering. */}
      {action && !streaming && !collapsed ? (
        <div className="flex items-center gap-2 border-t border-border/50 px-3 py-2">
          <Button
            type="button"
            size="sm"
            variant="secondary"
            disabled={action.disabled}
            onClick={action.onClick}
          >
            {action.label}
          </Button>
          {action.hint ? (
            <span className="min-w-0 flex-1 truncate text-[11px] text-muted-foreground">
              {action.hint}
            </span>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
