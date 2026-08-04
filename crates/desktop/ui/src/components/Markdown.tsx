import type { Components } from "react-markdown";
import { memo, useMemo, type ReactNode } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

import { CodeBlock } from "@/components/CodeBlock";
import { MermaidBlock } from "@/components/MermaidBlock";
import { linkClassName } from "@/lib/linkify";
import { splitBlocks } from "@/lib/markdownBlocks";
import { cn } from "@/lib/utils";

function codeText(children: ReactNode): string {
  if (typeof children === "string") return children;
  if (Array.isArray(children)) return children.map(codeText).join("");
  if (children == null || typeof children === "boolean") return "";
  if (typeof children === "object" && "props" in children) {
    const nested = (children as { props?: { children?: ReactNode } }).props
      ?.children;
    return codeText(nested);
  }
  return String(children);
}

function componentsFor(streaming: boolean): Components {
  return {
  p: ({ children }) => (
    <p className="mb-3 last:mb-0 leading-[1.65]">{children}</p>
  ),
  strong: ({ children }) => (
    <strong className="font-semibold text-foreground">{children}</strong>
  ),
  em: ({ children }) => <em className="italic">{children}</em>,
  a: ({ href, children }) => (
    <a href={href} target="_blank" rel="noreferrer" className={linkClassName}>
      {children}
    </a>
  ),
  ul: ({ children }) => (
    <ul className="mb-3 list-disc space-y-1 pl-5 last:mb-0">{children}</ul>
  ),
  ol: ({ children }) => (
    <ol className="mb-3 list-decimal space-y-1 pl-5 last:mb-0">{children}</ol>
  ),
  li: ({ children }) => <li className="leading-[1.65]">{children}</li>,
  h1: ({ children }) => (
    <h1 className="mb-2 mt-4 text-base font-semibold tracking-[-0.2px] first:mt-0">
      {children}
    </h1>
  ),
  h2: ({ children }) => (
    <h2 className="mb-2 mt-4 text-[15px] font-semibold tracking-[-0.2px] first:mt-0">
      {children}
    </h2>
  ),
  h3: ({ children }) => (
    <h3 className="mb-1.5 mt-3 text-sm font-semibold first:mt-0">{children}</h3>
  ),
  blockquote: ({ children }) => (
    <blockquote className="mb-3 border-l-2 border-border pl-3 text-muted-foreground last:mb-0">
      {children}
    </blockquote>
  ),
  code: ({ className, children }) => {
    const isBlock = Boolean(className?.includes("language-"));
    if (isBlock) {
      // Fenced blocks are handled by `pre` → CodeBlock.
      return <code className={className}>{children}</code>;
    }
    return (
      <code className="rounded-md bg-muted px-1 py-0.5 font-mono text-[12px] text-foreground">
        {children}
      </code>
    );
  },
  pre: ({ children }) => {
    const child = Array.isArray(children) ? children[0] : children;
    const props =
      child && typeof child === "object" && "props" in child
        ? (child as {
            props?: { className?: string; children?: ReactNode };
          }).props
        : undefined;
    const className = props?.className ?? "";
    const langMatch = /language-([\w+#.-]+)/.exec(className);
    const language = langMatch?.[1] ?? "plaintext";
    const code = codeText(props?.children ?? children).replace(/\n$/, "");
    return language.toLowerCase() === "mermaid" ? (
      <MermaidBlock code={code} streaming={streaming} />
    ) : (
      <CodeBlock code={code} language={language} />
    );
  },
  hr: () => <hr className="my-4 border-border/70" />,
  table: ({ children }) => (
    <div className="mb-3 overflow-x-auto last:mb-0">
      <table className="w-full border-collapse text-left text-[13px]">
        {children}
      </table>
    </div>
  ),
  th: ({ children }) => (
    <th className="border-b border-border px-2 py-1.5 font-medium text-foreground">
      {children}
    </th>
  ),
  td: ({ children }) => (
    <td className="border-b border-border/60 px-2 py-1.5 text-muted-foreground">
      {children}
    </td>
  ),
  };
}

type Props = {
  children: string;
  className?: string;
  streaming?: boolean;
};

/**
 * One top-level markdown block.
 *
 * Memoized separately from its neighbours: while a message streams, only its
 * trailing block changes, so every settled block above skips re-parsing
 * entirely. That is the difference between O(n²) and O(n) over a long answer.
 */
const Block = memo(function Block({ text, streaming }: { text: string; streaming: boolean }) {
  const components = useMemo(() => componentsFor(streaming), [streaming]);

  return (
    <ReactMarkdown remarkPlugins={[remarkGfm]} components={components}>
      {text}
    </ReactMarkdown>
  );
});

/**
 * GFM markdown for assistant (and muted thinking) bodies.
 *
 * Memoized twice over, and both levels are load-bearing rather than
 * micro-optimisations:
 *
 * - **This component** skips messages the reducer did not touch. Without it one
 *   streaming message re-parses every *finished* message in the transcript on
 *   every frame, so a long chat degrades as it grows.
 * - **Each block** skips the settled part of the message being streamed. A
 *   single growing string means re-parsing the whole document per frame; blocks
 *   mean re-parsing only the tail.
 */
export const Markdown = memo(function Markdown({
  children,
  className,
  streaming = false,
}: Props) {
  const blocks = useMemo(() => splitBlocks(children), [children]);

  return (
    <div
      className={cn(
        "max-w-none text-[15px] text-foreground wrap-break-word",
        className
      )}
    >
      {blocks.map((block) => (
        <Block key={block.key} text={block.text} streaming={streaming} />
      ))}
    </div>
  );
});
