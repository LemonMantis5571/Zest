import { useEffect, useId } from "react";
import { XIcon } from "lucide-react";

import { DiffPreview } from "@/components/CodeBlock";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

export type DiffViewerTarget = {
  path: string;
  diff: string;
};

type Props = {
  target: DiffViewerTarget | null;
  onClose: () => void;
};

/** Full-panel unified diff — portal-free overlay (WebView-safe). */
export function DiffViewer({ target, onClose }: Props) {
  const titleId = useId();

  useEffect(() => {
    if (!target) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [target, onClose]);

  if (!target) return null;

  return (
    <div className="absolute inset-0 z-50 flex flex-col overflow-hidden bg-black/55 animate-in fade-in duration-150">
      <button
        type="button"
        aria-label="Close diff"
        className="absolute inset-0 cursor-pointer"
        onClick={onClose}
      />
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        className={cn(
          "relative m-3 flex min-h-0 flex-1 flex-col overflow-hidden rounded-xl border border-border bg-[var(--chat-header,#121314)] shadow-2xl",
          "animate-in zoom-in-95 duration-150"
        )}
      >
        <header className="flex shrink-0 items-center justify-between gap-3 border-b border-border/60 px-4 py-2.5">
          <div className="min-w-0">
            <div id={titleId} className="text-sm font-semibold">
              Diff
            </div>
            <div
              className="truncate font-mono text-[11px] text-muted-foreground"
              title={target.path}
            >
              {target.path || "Untitled"}
            </div>
          </div>
          <Button type="button" variant="ghost" size="icon-sm" title="Close" onClick={onClose}>
            <XIcon />
          </Button>
        </header>
        <div className="min-h-0 flex-1 overflow-auto">
          <DiffPreview
            diff={target.diff}
            className="border-b-0"
            maxHeightClass="max-h-none"
          />
        </div>
      </div>
    </div>
  );
}
