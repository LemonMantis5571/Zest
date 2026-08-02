import { useEffect, useState } from "react";
import { CheckIcon, CopyIcon } from "lucide-react";

import { highlightCode, languageLabel, normalizeLang } from "@/lib/highlight";
import { cn } from "@/lib/utils";

type Props = {
  code: string;
  language?: string | null;
  className?: string;
  /** Show language chip in the header (default true). */
  showLang?: boolean;
};

/**
 * Editor-style fenced code: language chip, copy, Shiki highlight.
 * Falls back to plain mono while highlighting or if Shiki fails.
 */
export function CodeBlock({
  code,
  language,
  className,
  showLang = true,
}: Props) {
  const lang = normalizeLang(language);
  const label = languageLabel(lang);
  const [html, setHtml] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    let cancelled = false;
    highlightCode(code, lang)
      .then((next) => {
        if (!cancelled) setHtml(next);
      })
      .catch(() => {
        if (!cancelled) setHtml(null);
      });
    return () => {
      cancelled = true;
    };
  }, [code, lang]);

  async function copy() {
    try {
      await navigator.clipboard.writeText(code);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1400);
    } catch {
      /* ignore */
    }
  }

  return (
    <div
      className={cn(
        "group/code my-3 overflow-hidden rounded-xl border border-border/70 bg-[#0d1117] last:mb-0",
        className
      )}
    >
      <div className="flex items-center justify-between gap-2 border-b border-border/50 px-3 py-1.5">
        {showLang ? (
          <span className="font-mono text-[11px] uppercase tracking-wide text-muted-foreground">
            {label}
          </span>
        ) : (
          <span />
        )}
        <button
          type="button"
          onClick={() => void copy()}
          title={copied ? "Copied" : "Copy"}
          className={cn(
            "inline-flex size-7 items-center justify-center rounded-md text-muted-foreground outline-none transition-colors",
            "hover:bg-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/40"
          )}
        >
          {copied ? (
            <CheckIcon className="size-3.5 text-[var(--success,#27a644)]" />
          ) : (
            <CopyIcon className="size-3.5" />
          )}
        </button>
      </div>
      <div className="overflow-x-auto">
        {html ? (
          <div
            className="code-highlight [&_pre]:m-0 [&_pre]:bg-transparent! [&_pre]:p-3 [&_pre]:text-[12.5px] [&_pre]:leading-[1.65] [&_code]:font-mono [&_code]:text-[12.5px] [&_span]:text-[length:inherit]"
            dangerouslySetInnerHTML={{ __html: html }}
          />
        ) : (
          <pre className="m-0 p-3 font-mono text-[12.5px] leading-[1.65] text-muted-foreground whitespace-pre">
            {code}
          </pre>
        )}
      </div>
    </div>
  );
}

type DiffPreviewProps = {
  diff: string;
  path?: string;
  className?: string;
};

/** Live edit preview for write approvals — colored +/- lines. */
export function DiffPreview({ diff, path, className }: DiffPreviewProps) {
  const lines = diff.split("\n");

  return (
    <div
      className={cn(
        "overflow-hidden border-b border-border/60 bg-[#0d1117]",
        className
      )}
    >
      {path ? (
        <div className="border-b border-border/40 px-3 py-1.5 font-mono text-[11px] text-muted-foreground">
          {path}
        </div>
      ) : null}
      <pre className="max-h-56 overflow-auto p-0 font-mono text-[11.5px] leading-[1.6]">
        {lines.map((line, i) => {
          const kind =
            line.startsWith("+") && !line.startsWith("+++")
              ? "add"
              : line.startsWith("-") && !line.startsWith("---")
                ? "del"
                : line.startsWith("@@")
                  ? "hunk"
                  : "ctx";
          return (
            <div
              key={`${i}:${line.slice(0, 24)}`}
              className={cn(
                "px-3 whitespace-pre-wrap break-all",
                kind === "add" && "bg-[rgba(39,166,68,0.12)] text-[#3fb950]",
                kind === "del" && "bg-[rgba(229,72,77,0.12)] text-[#f85149]",
                kind === "hunk" && "bg-[rgba(94,106,210,0.12)] text-[#828fff]",
                kind === "ctx" && "text-muted-foreground"
              )}
            >
              {line || " "}
            </div>
          );
        })}
      </pre>
    </div>
  );
}
