import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import {
  CheckIcon,
  ChevronDownIcon,
  LoaderCircleIcon,
  SparklesIcon,
} from "lucide-react";

import {
  thinkingSummaryLabel,
  thinkingTraceRows,
} from "@/lib/thinkingSummary";
import { cn } from "@/lib/utils";

const MAX_VISIBLE_ROWS = 7;

type ThinkingTraceProps = {
  thinking: string;
  streaming: boolean;
  emptyLabel?: string;
};

export function ThinkingTrace({
  thinking,
  streaming,
  emptyLabel = "Working...",
}: ThinkingTraceProps) {
  const rows = useMemo(() => thinkingTraceRows(thinking), [thinking]);
  const [showAllRows, setShowAllRows] = useState(false);
  const visibleRows = useMemo(
    () => (showAllRows ? rows : rows.slice(-MAX_VISIBLE_ROWS)),
    [rows, showAllRows]
  );
  const earlierCount = rows.length - visibleRows.length;
  const [expanded, setExpanded] = useState(streaming);
  const wasStreaming = useRef(streaming);
  const traceRef = useRef<HTMLDivElement>(null);
  const [lineHeight, setLineHeight] = useState(0);

  useEffect(() => {
    if (wasStreaming.current !== streaming) setExpanded(streaming);
    wasStreaming.current = streaming;
  }, [streaming]);

  useLayoutEffect(() => {
    if (!expanded || !traceRef.current) {
      setLineHeight(0);
      return;
    }
    setLineHeight(traceRef.current.offsetHeight);
  }, [expanded, earlierCount, rows, showAllRows, thinking]);

  const label = streaming ? "Thinking" : thinkingSummaryLabel(thinking);

  return (
    <div className="min-w-0 text-xs text-muted-foreground">
      <button
        type="button"
        aria-expanded={expanded}
        aria-label={expanded ? "Hide thinking trace" : "Show thinking trace"}
        onClick={() => setExpanded((value) => !value)}
        className="flex max-w-full cursor-pointer items-center gap-1.5 rounded-md py-0.5 text-left outline-none transition-colors hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/40"
      >
        <SparklesIcon
          className={cn(
            "size-3.5 shrink-0 text-muted-foreground/80",
            streaming && "text-primary/80"
          )}
          aria-hidden
        />
        <span
          key={`${label}-${streaming}`}
          aria-live="polite"
          className={cn(
            "min-w-0 truncate font-medium animate-in fade-in duration-300",
            streaming && "shimmer-text"
          )}
        >
          {label}
        </span>
        <ChevronDownIcon
          className={cn(
            "size-3 shrink-0 text-muted-foreground/60 transition-transform duration-200",
            expanded && "rotate-180"
          )}
          aria-hidden
        />
      </button>

      <div
        className={cn(
          "grid transition-[grid-template-rows,opacity] duration-300 ease-out",
          expanded ? "grid-rows-[1fr] opacity-100" : "grid-rows-[0fr] opacity-0"
        )}
      >
        <div className="min-h-0 overflow-hidden">
          <div className="relative ml-1 mt-1 pl-4">
            {lineHeight > 0 ? (
              <span
                aria-hidden
                className="absolute left-[3px] top-1 w-px bg-border/70 transition-[height] duration-300"
                style={{ height: Math.max(0, lineHeight - 8) }}
              />
            ) : null}

            <div
              ref={traceRef}
              className="flex max-h-52 flex-col gap-0.5 overflow-y-auto py-1 pr-1"
            >
              {earlierCount > 0 ? (
                <button
                  type="button"
                  onClick={() => setShowAllRows(true)}
                  className="w-fit rounded px-1.5 py-1 text-left text-[11px] text-muted-foreground/60 transition-colors hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/40"
                >
                  +{earlierCount} earlier step{earlierCount === 1 ? "" : "s"}
                </button>
              ) : null}

              {visibleRows.length > 0 ? (
                visibleRows.map((row, index) => {
                  const active = streaming && index === visibleRows.length - 1;
                  return (
                    <div
                      key={`${row.primary}-${index}`}
                      className="flex min-w-0 items-start gap-2 rounded-md px-1.5 py-1 animate-in fade-in slide-in-from-bottom-1 fill-mode-both duration-300"
                      style={{ animationDelay: `${Math.min(index * 35, 210)}ms` }}
                    >
                      <span className="mt-0.5 flex size-3.5 shrink-0 items-center justify-center">
                        {row.kind === "step" ? (
                          active ? (
                            <LoaderCircleIcon
                              className="size-3.5 animate-spin text-primary/80"
                              aria-hidden
                            />
                          ) : (
                            <CheckIcon
                              className="size-3.5 text-muted-foreground/70"
                              aria-hidden
                            />
                          )
                        ) : (
                          <span
                            className="size-1.5 rounded-full bg-muted-foreground/60"
                            aria-hidden
                          />
                        )}
                      </span>
                      <span className="min-w-0 flex-1">
                        <span
                          className={cn(
                            "block truncate leading-5",
                            row.kind === "step"
                              ? "text-foreground/85"
                              : "text-muted-foreground"
                          )}
                        >
                          {row.primary}
                        </span>
                        {row.secondary ? (
                          <span className="block truncate text-[11px] leading-4 text-muted-foreground/65">
                            {row.secondary}
                          </span>
                        ) : null}
                      </span>
                    </div>
                  );
                })
              ) : (
                <div className="px-1.5 py-1 text-muted-foreground/70">
                  {emptyLabel}
                </div>
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
