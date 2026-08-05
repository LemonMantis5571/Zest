import { useEffect, useId, useState } from "react";
import { XIcon } from "lucide-react";

import { DiffPreview } from "@/components/CodeBlock";
import { Button } from "@/components/ui/button";
import { generateReadingDiff, type ReadingDiffView } from "@/lib/api";
import { makeReadingDiff, type ReadingDiff } from "@/lib/readingDiff";
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
  const [view, setView] = useState<"reading" | "full">("reading");
  const [reading, setReading] = useState<ReadingDiff | ReadingDiffView | null>(null);

  useEffect(() => {
    if (!target) return;
    setView("reading");
    const fallback = makeReadingDiff(target.diff);
    setReading(fallback);
    let cancelled = false;
    void generateReadingDiff(target.diff)
      .then((result) => {
        if (!cancelled) setReading(result);
      })
      .catch(() => {
        // The local conservative view remains useful when the provider is
        // unavailable or returns an invalid plan.
      });
    return () => {
      cancelled = true;
    };
  }, [target]);

  useEffect(() => {
    if (!target) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [target, onClose]);

  if (!target) return null;

  const hiddenCount = reading && "hiddenImports" in reading ? reading.hiddenImports : 0;
  const foldedCount = reading && "foldedContextLines" in reading
    ? reading.foldedContextLines
    : reading?.foldedLines ?? 0;

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
          <div className="flex shrink-0 rounded-md border border-border/60 p-0.5">
            <button
              type="button"
              className={cn(
                "rounded px-2 py-1 text-[11px] transition-colors",
                view === "reading"
                  ? "bg-accent text-foreground"
                  : "text-muted-foreground hover:text-foreground"
              )}
              onClick={() => setView("reading")}
            >
              Reading
            </button>
            <button
              type="button"
              className={cn(
                "rounded px-2 py-1 text-[11px] transition-colors",
                view === "full"
                  ? "bg-accent text-foreground"
                  : "text-muted-foreground hover:text-foreground"
              )}
              onClick={() => setView("full")}
            >
              Full
            </button>
          </div>
          <Button type="button" variant="ghost" size="icon-sm" title="Close" onClick={onClose}>
            <XIcon />
          </Button>
        </header>
        {view === "reading" && reading ? (
          <div className="flex shrink-0 items-center gap-2 border-b border-border/50 px-4 py-1.5 text-[11px] text-muted-foreground">
            <span>Display-only review view</span>
            {hiddenCount > 0 ? (
              <span>· {hiddenCount} import lines hidden</span>
            ) : null}
            {foldedCount > 0 ? (
              <span>· {foldedCount} context lines folded</span>
            ) : null}
          </div>
        ) : null}
        <div className="min-h-0 flex-1 overflow-auto">
          <DiffPreview
            diff={view === "reading" && reading ? reading.diff : target.diff}
            className="border-b-0"
            maxHeightClass="max-h-none"
          />
        </div>
      </div>
    </div>
  );
}
