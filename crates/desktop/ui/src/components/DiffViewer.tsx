import { useEffect, useId, useMemo, useState } from "react";
import {
  ChevronDownIcon,
  GitBranchIcon,
  XIcon,
} from "lucide-react";

import { DiffPreview } from "@/components/CodeBlock";
import { Button } from "@/components/ui/button";
import { generateReadingDiff, type ReadingDiffView } from "@/lib/api";
import { splitDiffSections, type DiffSection } from "@/lib/diffSections";
import { makeReadingDiff, type ReadingDiff } from "@/lib/readingDiff";
import { cn } from "@/lib/utils";

export type DiffViewerTarget = {
  path: string;
  diff: string;
};

type Props = {
  target: DiffViewerTarget | null;
  onClose: () => void;
  branch?: string | null;
  baseBranch?: string | null;
};

type DiffView = "reading" | "full";

function stripDiffMetadata(diff: string): string {
  return diff
    .split("\n")
    .filter(
      (line) =>
        !/^(?:diff --git |index |--- (?:a\/|b\/|\/dev\/null)|\+\+\+ (?:a\/|b\/|\/dev\/null)|old mode |new mode |similarity |rename from |rename to |copy from |copy to )/.test(
          line
        )
    )
    .join("\n");
}

function sectionKey(section: DiffSection, index: number): string {
  return `${section.path}:${index}`;
}

/** Compact review sidebar for changed files — portal-free for WebView safety. */
export function DiffViewer({ target, onClose, branch, baseBranch }: Props) {
  const titleId = useId();
  const [view, setView] = useState<DiffView>("reading");
  const [reading, setReading] = useState<ReadingDiff | ReadingDiffView | null>(null);
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());

  useEffect(() => {
    if (!target) return;
    setView("reading");
    setCollapsed(new Set());
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
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [target, onClose]);

  const activeDiff =
    target && view === "reading" && reading ? reading.diff : target?.diff ?? "";
  const sections = useMemo(
    () => splitDiffSections(activeDiff, target?.path ?? ""),
    [activeDiff, target?.path]
  );
  const hiddenCount = reading && "hiddenImports" in reading ? reading.hiddenImports : 0;
  const foldedCount =
    reading && "foldedContextLines" in reading
      ? reading.foldedContextLines
      : reading?.foldedLines ?? 0;
  const totalAdded = sections.reduce((sum, section) => sum + section.added, 0);
  const totalRemoved = sections.reduce((sum, section) => sum + section.removed, 0);
  const hasBranchContext = Boolean(branch || baseBranch);

  function toggleSection(key: string) {
    setCollapsed((current) => {
      const next = new Set(current);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }

  if (!target) return null;

  return (
    <div className="pointer-events-none absolute inset-0 z-50 flex justify-end p-2 sm:p-3">
      <aside
        role="dialog"
        aria-modal="false"
        aria-labelledby={titleId}
        className="pointer-events-auto flex h-full w-[min(520px,calc(100%_-_16px))] min-w-0 flex-col overflow-hidden rounded-xl border border-border/80 bg-[#0f1011] text-foreground shadow-2xl outline-none animate-in slide-in-from-right-2 duration-150"
      >
        <header className="shrink-0 border-b border-border/70 bg-[#141516]">
          <div className="flex items-start justify-between gap-3 px-3 py-2.5">
            <div className="min-w-0">
              <div id={titleId} className="flex items-center gap-1.5 text-xs font-medium">
                <GitBranchIcon className="size-3.5 text-primary/80" aria-hidden="true" />
                <span>{hasBranchContext ? "Branch changes" : "File changes"}</span>
                <ChevronDownIcon className="size-3 text-muted-foreground/70" aria-hidden="true" />
              </div>
              <div
                className="mt-1 truncate font-mono text-[10px] text-muted-foreground"
                title={hasBranchContext ? `${baseBranch ?? "base"} → ${branch ?? "current"}` : target.path}
              >
                {hasBranchContext
                  ? `${baseBranch ?? "base"} → ${branch ?? "current"}`
                  : `${sections.length} ${sections.length === 1 ? "file" : "files"} changed`}
              </div>
            </div>
            <div className="flex shrink-0 items-center gap-1">
              <div className="flex items-center rounded-md border border-border/70 bg-background/30 p-0.5">
                <button
                  type="button"
                  className={cn(
                    "rounded px-2 py-1 text-[10px] transition-colors",
                    view === "reading"
                      ? "bg-secondary text-foreground"
                      : "text-muted-foreground hover:text-foreground"
                  )}
                  onClick={() => setView("reading")}
                >
                  Clean
                </button>
                <button
                  type="button"
                  className={cn(
                    "rounded px-2 py-1 text-[10px] transition-colors",
                    view === "full"
                      ? "bg-secondary text-foreground"
                      : "text-muted-foreground hover:text-foreground"
                  )}
                  onClick={() => setView("full")}
                >
                  Raw
                </button>
              </div>
              <Button
                type="button"
                variant="ghost"
                size="icon-sm"
                title="Close changes"
                aria-label="Close changes"
                onClick={onClose}
              >
                <XIcon />
              </Button>
            </div>
          </div>
          <div className="flex items-center gap-2 border-t border-border/50 px-3 py-1.5 text-[11px]">
            <span className="text-muted-foreground">
              {sections.length} {sections.length === 1 ? "file" : "files"}
            </span>
            <span className="text-[#3fb950]">+{totalAdded}</span>
            <span className="text-[#f85149]">−{totalRemoved}</span>
          </div>
          {view === "reading" && reading && (hiddenCount > 0 || foldedCount > 0) ? (
            <div className="border-t border-border/50 px-3 py-1.5 text-[10px] text-muted-foreground/75">
              Clean view
              {hiddenCount > 0 ? ` · ${hiddenCount} import lines hidden` : null}
              {foldedCount > 0 ? ` · ${foldedCount} lines folded` : null}
            </div>
          ) : null}
        </header>

        <div className="min-h-0 flex-1 overflow-y-auto bg-[#0b0c0d]">
          {sections.length > 0 ? (
            sections.map((section, index) => {
              const key = sectionKey(section, index);
              const isCollapsed = collapsed.has(key);
              const displayDiff = view === "reading" ? stripDiffMetadata(section.diff) : section.diff;
              return (
                <section key={key} className="border-b border-border/60 last:border-b-0">
                  <button
                    type="button"
                    aria-expanded={!isCollapsed}
                    className="flex w-full min-w-0 items-center gap-1.5 px-3 py-2 text-left transition-colors hover:bg-secondary/35 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/40"
                    onClick={() => toggleSection(key)}
                  >
                    <ChevronDownIcon
                      className={cn(
                        "size-3 shrink-0 text-muted-foreground/70 transition-transform duration-150",
                        isCollapsed && "-rotate-90"
                      )}
                      aria-hidden="true"
                    />
                    <span className="min-w-0 flex-1 truncate font-mono text-[11px] font-medium text-foreground/90" title={section.path}>
                      {section.path}
                    </span>
                    <span className="shrink-0 font-mono text-[10px] text-[#3fb950]">+{section.added}</span>
                    <span className="shrink-0 font-mono text-[10px] text-[#f85149]">−{section.removed}</span>
                  </button>
                  {!isCollapsed ? (
                    <DiffPreview
                      diff={displayDiff}
                      className="border-b-0 rounded-none bg-[#0b0c0d]"
                      maxHeightClass="max-h-none"
                    />
                  ) : null}
                </section>
              );
            })
          ) : (
            <div className="px-3 py-8 text-center text-xs text-muted-foreground">
              No changes to show.
            </div>
          )}
        </div>
      </aside>
    </div>
  );
}
