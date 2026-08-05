import {
  CheckCircle2Icon,
  ChevronRightIcon,
  Clock3Icon,
  GitForkIcon,
  HistoryIcon,
  ListTreeIcon,
  PanelRightCloseIcon,
  RefreshCwIcon,
  TriangleAlertIcon,
  WrenchIcon,
  XCircleIcon,
} from "lucide-react";
import { useMemo, useState } from "react";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import type { ChatMessage, SessionInfo } from "@/lib/types";

type Props = {
  open: boolean;
  session: SessionInfo;
  messages: ChatMessage[];
  sending: boolean;
  compacting: boolean;
  onClose: () => void;
  onFork: () => Promise<void>;
  onRewind: (checkpointId: string) => Promise<void>;
  onJump: (messageId: string) => void;
};

type Tab = "activity" | "outline";

function formatAge(epochSecs: number) {
  const delta = Math.max(0, Math.floor(Date.now() / 1000) - epochSecs);
  if (delta < 60) return "just now";
  if (delta < 3600) return `${Math.floor(delta / 60)}m ago`;
  if (delta < 86400) return `${Math.floor(delta / 3600)}h ago`;
  return `${Math.floor(delta / 86400)}d ago`;
}

function statusIcon(status: string) {
  if (status === "done") return <CheckCircle2Icon className="text-primary" />;
  if (status === "error") return <XCircleIcon className="text-destructive" />;
  if (status === "awaiting_approval") {
    return <TriangleAlertIcon className="text-amber-400" />;
  }
  return <RefreshCwIcon className="animate-spin text-primary" />;
}

function messagePreview(message: ChatMessage) {
  const text = message.text.trim().replace(/\s+/g, " ");
  if (text) return text.slice(0, 96) + (text.length > 96 ? "…" : "");
  if (message.role === "assistant" && message.tools.length) {
    return `${message.tools.length} tool ${message.tools.length === 1 ? "call" : "calls"}`;
  }
  return message.role === "user" ? "Attachment" : "Working…";
}

export function WorkbenchPanel({
  open,
  session,
  messages,
  sending,
  compacting,
  onClose,
  onFork,
  onRewind,
  onJump,
}: Props) {
  const [tab, setTab] = useState<Tab>("activity");
  const [busyAction, setBusyAction] = useState<"fork" | string | null>(null);

  const tasks = useMemo(
    () =>
      messages
        .flatMap((message) =>
          message.role === "assistant"
            ? message.tools.map((tool) => ({ ...tool, messageId: message.id }))
            : []
        )
        .reverse(),
    [messages]
  );

  const outline = useMemo(
    () => messages.filter((message) => message.text.trim() || message.role === "assistant"),
    [messages]
  );

  async function runFork() {
    setBusyAction("fork");
    try {
      await onFork();
    } finally {
      setBusyAction(null);
    }
  }

  async function runRewind(id: string) {
    setBusyAction(id);
    try {
      await onRewind(id);
    } finally {
      setBusyAction(null);
    }
  }

  if (!open) return null;

  return (
    <aside className="absolute inset-y-0 right-0 z-30 flex w-[min(360px,calc(100%-24px))] min-w-0 flex-col border-l border-border bg-background text-foreground">
      <header className="flex shrink-0 items-center justify-between border-b border-border/60 px-4 py-3">
        <div>
          <div className="flex items-center gap-2 text-sm font-semibold">
            <WrenchIcon className="size-4 text-primary" />
            Workbench
          </div>
          <div className="mt-0.5 text-[11px] text-muted-foreground">
            Review your work and return to earlier points.
          </div>
        </div>
        <Button type="button" variant="ghost" size="icon-sm" title="Close workbench" onClick={onClose}>
          <PanelRightCloseIcon />
        </Button>
      </header>

      <div className="grid grid-cols-2 gap-1 border-b border-border/60 p-1.5">
        {([
          ["activity", "Activity", Clock3Icon],
          ["outline", "Outline", ListTreeIcon],
        ] as const).map(([id, label, Icon]) => (
          <button
            key={id}
            type="button"
            className={cn(
              "flex items-center justify-center gap-1.5 rounded-md px-2 py-1.5 text-xs transition-colors",
              tab === id
                ? "bg-secondary text-foreground"
                : "text-muted-foreground hover:bg-secondary/60 hover:text-foreground"
            )}
            onClick={() => setTab(id)}
          >
            <Icon className="size-3.5" />
            {label}
          </button>
        ))}
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-3 py-3">
        {tab === "activity" ? (
          <div className="flex flex-col gap-3">
            <section className="rounded-xl border border-border/70 bg-background/30 p-3">
              <div className="flex items-center justify-between gap-3">
                <div>
                  <div className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
                    Current session
                  </div>
                  <div className="mt-1 truncate text-sm font-medium">{session.label}</div>
                </div>
                <span
                  className={cn(
                    "inline-flex items-center gap-1.5 rounded-full px-2 py-1 text-[10px] font-medium",
                    sending || compacting
                      ? "bg-primary/12 text-primary"
                      : "bg-secondary text-muted-foreground"
                  )}
                >
                  <span className={cn("size-1.5 rounded-full", sending || compacting ? "bg-primary" : "bg-muted-foreground")} />
                  {sending ? "Working" : compacting ? "Compacting" : "Ready"}
                </span>
              </div>
              <div className="mt-3 grid grid-cols-2 gap-2 text-[11px]">
                <div className="rounded-md bg-secondary/50 px-2 py-1.5">
                  <div className="text-muted-foreground">Model</div>
                  <div className="mt-0.5 truncate font-mono text-foreground">{session.model}</div>
                </div>
                <div className="rounded-md bg-secondary/50 px-2 py-1.5">
                  <div className="text-muted-foreground">Messages</div>
                  <div className="mt-0.5 font-mono text-foreground">{messages.length}</div>
                </div>
              </div>
            </section>

            <section>
              <div className="mb-1.5 flex items-center justify-between px-1">
                <h2 className="m-0 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
                  Work units
                </h2>
                <span className="text-[10px] text-muted-foreground">{tasks.length}</span>
              </div>
              {tasks.length === 0 ? (
                <div className="rounded-xl border border-dashed border-border/70 px-3 py-4 text-center text-xs text-muted-foreground">
                  Tool activity and approvals will appear here during a turn.
                </div>
              ) : (
                <div className="flex flex-col gap-1.5">
                  {tasks.slice(0, 12).map((task) => (
                    <button
                      type="button"
                      key={task.id}
                      className="flex w-full items-start gap-2 rounded-lg border border-border/60 bg-background/25 px-2.5 py-2 text-left transition-colors hover:bg-secondary/60"
                      onClick={() => onJump(task.messageId)}
                    >
                      <span className="mt-0.5 shrink-0">{statusIcon(task.status)}</span>
                      <span className="min-w-0 flex-1">
                        <span className="flex items-center gap-1.5">
                          <span className="truncate font-mono text-[11px]">{task.name}</span>
                          <ChevronRightIcon className="size-3 shrink-0 text-muted-foreground" />
                        </span>
                        <span className="mt-0.5 block truncate text-[10px] text-muted-foreground">
                          {task.summary || task.path || task.status.replaceAll("_", " ")}
                        </span>
                      </span>
                    </button>
                  ))}
                </div>
              )}
            </section>

            <section>
              <div className="mb-1.5 flex items-center justify-between px-1">
                <h2 className="m-0 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
                  Recovery
                </h2>
                <span className="text-[10px] text-muted-foreground">
                  {session.checkpoints.length} checkpoints
                </span>
              </div>
              <div className="flex flex-col gap-1.5">
                <Button type="button" variant="outline" size="sm" disabled={sending || busyAction !== null} onClick={() => void runFork()}>
                  <GitForkIcon data-icon="inline-start" />
                  Create separate conversation
                </Button>
              </div>
              {session.checkpoints.length ? (
                <div className="mt-2 flex flex-col gap-1">
                  {session.checkpoints
                    .slice()
                    .reverse()
                    .slice(0, 6)
                    .map((checkpoint) => (
                      <div key={checkpoint.id} className="flex items-center gap-2 rounded-md px-2 py-1.5 hover:bg-secondary/50">
                        <HistoryIcon className="size-3.5 shrink-0 text-muted-foreground" />
                        <span className="min-w-0 flex-1">
                          <span className="block truncate text-[11px]">{checkpoint.label}</span>
                          <span className="block text-[10px] text-muted-foreground">
                            {checkpoint.messageCount} messages · {formatAge(checkpoint.createdAt)}
                          </span>
                        </span>
                        <Button
                          type="button"
                          variant="ghost"
                          size="icon-xs"
                          title={`Rewind to ${checkpoint.label}`}
                          disabled={sending || busyAction !== null}
                          onClick={() => void runRewind(checkpoint.id)}
                        >
                          <RefreshCwIcon />
                        </Button>
                      </div>
                    ))}
                </div>
              ) : null}
            </section>
          </div>
        ) : (
          <section>
            <div className="mb-1.5 px-1 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
              Transcript map
            </div>
            {outline.length === 0 ? (
              <div className="rounded-xl border border-dashed border-border/70 px-3 py-4 text-center text-xs text-muted-foreground">
                Your conversation outline will appear here.
              </div>
            ) : (
              <div className="flex flex-col gap-1">
                {outline.map((message, index) => (
                  <button
                    type="button"
                    key={message.id}
                    className="group flex w-full items-start gap-2 rounded-lg px-2.5 py-2 text-left transition-colors hover:bg-secondary/60"
                    onClick={() => onJump(message.id)}
                  >
                    <span className="mt-0.5 w-5 shrink-0 text-right font-mono text-[10px] text-muted-foreground">
                      {index + 1}
                    </span>
                    <span className="min-w-0 flex-1">
                      <span className="block text-[10px] uppercase tracking-wide text-muted-foreground">
                        {message.role === "user" ? "You" : "Zest"}
                      </span>
                      <span className="mt-0.5 block text-xs leading-relaxed text-foreground/85">
                        {messagePreview(message)}
                      </span>
                    </span>
                    <ChevronRightIcon className="mt-1 size-3 shrink-0 text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100" />
                  </button>
                ))}
              </div>
            )}
          </section>
        )}
      </div>
    </aside>
  );
}
