import {
  CheckCircle2Icon,
  BotIcon,
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
import { useEffect, useId, useMemo, useRef, useState } from "react";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import type { ChatMessage, SessionInfo, WorkspaceReview } from "@/lib/types";

type Props = {
  open: boolean;
  autoOpened?: boolean;
  session: SessionInfo;
  messages: ChatMessage[];
  sending: boolean;
  compacting: boolean;
  review: WorkspaceReview | null;
  onClose: () => void;
  onFork: () => Promise<void>;
  onVerify: () => Promise<void>;
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

function subagentLabel(id: string) {
  if (id === "claude") return "Claude Code";
  if (id === "gemini") return "Gemini CLI";
  return id
    .replace(/[-_]+/g, " ")
    .replace(/\b\w/g, (letter) => letter.toUpperCase());
}

function subagentStatus(status: string) {
  if (status === "running") return "Working";
  if (status === "awaiting_approval") return "Needs approval";
  if (status === "error") return "Failed";
  return "Done";
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
  autoOpened = false,
  session,
  messages,
  sending,
  compacting,
  review,
  onClose,
  onFork,
  onVerify,
  onRewind,
  onJump,
}: Props) {
  const [tab, setTab] = useState<Tab>("activity");
  const [busyAction, setBusyAction] = useState<"fork" | string | null>(null);
  const panelRef = useRef<HTMLElement>(null);
  const titleId = useId();
  const descriptionId = useId();

  useEffect(() => {
    if (!open) return;
    if (!autoOpened) panelRef.current?.focus();

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        onClose();
      }
    };
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [autoOpened, open, onClose]);

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

  const subagents = useMemo(() => {
    const latest = new Map<
      string,
      {
        id: string;
        label: string;
        model: string;
        status: string;
        messageId: string;
      }
    >();
    for (const task of tasks) {
      if (task.name !== "delegate_external" || task.metadata?.kind !== "delegation") {
        continue;
      }
      const id = task.metadata.provider_id;
      if (latest.has(id)) continue;
      latest.set(id, {
        id,
        label: subagentLabel(id),
        model: task.metadata.model,
        status: task.status,
        messageId: task.messageId,
      });
    }
    return [...latest.values()];
  }, [tasks]);

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

  async function runVerify() {
    setBusyAction("verify");
    try {
      await onVerify();
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
    <div className="pointer-events-none absolute inset-0 z-30 flex items-center justify-end p-3 sm:p-4">
      <button
        type="button"
        aria-label="Close Workbench"
        className="pointer-events-auto absolute inset-0 cursor-default"
        tabIndex={-1}
        onClick={onClose}
      />
      <aside
        ref={panelRef}
        id="workbench-panel"
        role="dialog"
        aria-labelledby={titleId}
        aria-describedby={descriptionId}
        tabIndex={-1}
        className="pointer-events-auto relative z-10 flex h-full max-h-[720px] w-[min(360px,calc(100%_-_24px))] min-w-0 flex-col overflow-hidden rounded-xl border border-border/70 bg-card text-card-foreground outline-none"
      >
      <header className="flex shrink-0 items-center justify-between border-b border-border/60 px-3 py-2.5">
        <div>
          <h2 id={titleId} className="flex items-center gap-2 text-sm font-semibold">
            <WrenchIcon className="size-4 text-primary" aria-hidden="true" />
            Workbench
          </h2>
          <p id={descriptionId} className="mt-0.5 text-[11px] text-muted-foreground">
            Review your work and return to earlier points.
          </p>
        </div>
        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          title="Close Workbench"
          aria-label="Close Workbench"
          onClick={onClose}
        >
          <PanelRightCloseIcon aria-hidden="true" />
        </Button>
      </header>

      <div
        role="tablist"
        aria-label="Workbench views"
        className="grid grid-cols-2 gap-1 border-b border-border/60 p-1.5"
      >
        {([
          ["activity", "Activity", Clock3Icon],
          ["outline", "Outline", ListTreeIcon],
        ] as const).map(([id, label, Icon]) => (
          <button
            key={id}
            type="button"
            id={`workbench-tab-${id}`}
            role="tab"
            aria-selected={tab === id}
            aria-controls="workbench-content"
            tabIndex={tab === id ? 0 : -1}
            className={cn(
              "flex items-center justify-center gap-1.5 rounded-md px-2 py-1.5 text-xs transition-colors",
              tab === id
                ? "bg-secondary text-foreground"
                : "text-muted-foreground hover:bg-secondary/60 hover:text-foreground"
            )}
            onClick={() => setTab(id)}
            onKeyDown={(event) => {
              if (!["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Home", "End"].includes(event.key)) {
                return;
              }
              event.preventDefault();
              const next: Tab =
                event.key === "Home"
                  ? "activity"
                  : event.key === "End"
                    ? "outline"
                    : id === "activity"
                      ? "outline"
                      : "activity";
              setTab(next);
              requestAnimationFrame(() => {
                document.getElementById(`workbench-tab-${next}`)?.focus();
              });
            }}
          >
            <Icon className="size-3.5" aria-hidden="true" />
            {label}
          </button>
        ))}
      </div>

      <div
        id="workbench-content"
        role="tabpanel"
        aria-labelledby={`workbench-tab-${tab}`}
        tabIndex={0}
        className="min-h-0 flex-1 overflow-y-auto px-2.5 py-2.5 outline-none"
      >
        {tab === "activity" ? (
          <div className="flex flex-col gap-2.5">
            <section className="border-b border-border/60 pb-3">
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
              <div className="mt-2.5 grid grid-cols-2 gap-1.5 text-[11px]">
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

            {subagents.length ? (
              <section className="border-b border-border/60 pb-2.5">
                <div className="mb-1.5 flex items-center justify-between px-1">
                  <h2 className="m-0 flex items-center gap-1.5 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
                    <BotIcon className="size-3.5" aria-hidden="true" />
                    Subagents
                  </h2>
                  <span className="text-[10px] text-muted-foreground">{subagents.length}</span>
                </div>
                <div className="flex flex-col gap-1.5">
                  {subagents.map((subagent) => (
                    <button
                      type="button"
                      key={subagent.id}
                      className="group flex w-full items-center gap-2 border-b border-border/60 px-1 py-2 text-left transition-colors last:border-b-0 hover:bg-secondary/40"
                      onClick={() => onJump(subagent.messageId)}
                      aria-label={`${subagent.label}, ${subagentStatus(subagent.status)}`}
                    >
                      <span className="shrink-0" aria-hidden="true">
                        {statusIcon(subagent.status)}
                      </span>
                      <span className="min-w-0 flex-1">
                        <span className="block truncate text-xs font-medium">{subagent.label}</span>
                        <span className="mt-0.5 block truncate text-[10px] text-muted-foreground">
                          {subagent.model} / {subagentStatus(subagent.status)}
                        </span>
                      </span>
                      <ChevronRightIcon className="size-3 shrink-0 text-muted-foreground" aria-hidden="true" />
                    </button>
                  ))}
                </div>
              </section>
            ) : null}

            <section className="border-b border-border/60 pb-3">
              <div className="flex items-start justify-between gap-3">
                <div className="min-w-0">
                  <div className="flex items-center gap-1.5 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
                    {review?.patchCheck === "issues" ? (
                      <TriangleAlertIcon className="size-3.5 text-amber-400" aria-hidden="true" />
                    ) : review ? (
                      <CheckCircle2Icon className="size-3.5 text-primary" aria-hidden="true" />
                    ) : null}
                    Workspace check
                  </div>
                  <div className="mt-1 text-xs text-foreground">
                    {review?.summary ?? "Review Git changes without changing files."}
                  </div>
                </div>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  disabled={sending || compacting || busyAction !== null}
                  onClick={() => void runVerify()}
                >
                  <RefreshCwIcon
                    data-icon="inline-start"
                    className={cn(busyAction === "verify" && "animate-spin")}
                    aria-hidden="true"
                  />
                  {review ? "Run again" : "Verify"}
                </Button>
              </div>
              {review ? (
                <div className="mt-3 flex flex-col gap-2 text-[11px]">
                  <div className="flex items-center justify-between gap-3 rounded-md bg-secondary/50 px-2 py-1.5">
                    <span className="text-muted-foreground">Patch check</span>
                    <span
                      className={cn(
                        "font-medium",
                        review.patchCheck === "clean"
                          ? "text-primary"
                          : review.patchCheck === "issues"
                            ? "text-amber-400"
                            : "text-muted-foreground"
                      )}
                    >
                      {review.patchCheck === "clean"
                        ? "Clear"
                        : review.patchCheck === "issues"
                          ? "Review needed"
                          : "Unavailable"}
                    </span>
                  </div>
                  {review.changedFiles.length > 0 ? (
                    <div className="flex flex-col gap-1 rounded-md bg-secondary/50 px-2 py-1.5">
                      <div className="text-muted-foreground">
                        Changed files ({review.changedFileCount})
                      </div>
                      <ul className="flex flex-col gap-0.5 font-mono text-[10px] text-foreground/80">
                        {review.changedFiles.slice(0, 5).map((file) => (
                          <li key={file} className="truncate" title={file}>
                            {file}
                          </li>
                        ))}
                      </ul>
                      {review.changedFileCount > review.changedFiles.length ? (
                        <div className="text-[10px] text-muted-foreground">
                          +{review.changedFileCount - review.changedFiles.length} more
                        </div>
                      ) : null}
                    </div>
                  ) : null}
                </div>
              ) : null}
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
                      className="group flex w-full items-start gap-2 border-b border-border/60 px-1 py-2 text-left transition-colors last:border-b-0 hover:bg-secondary/40"
                      onClick={() => onJump(task.messageId)}
                    >
                      <span className="mt-0.5 shrink-0" aria-hidden="true">{statusIcon(task.status)}</span>
                      <span className="min-w-0 flex-1">
                        <span className="flex items-center gap-1.5">
                          <span className="truncate font-mono text-[11px]">{task.name}</span>
                          <ChevronRightIcon className="size-3 shrink-0 text-muted-foreground" aria-hidden="true" />
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
                  Fork Conversation
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
                        <HistoryIcon className="size-3.5 shrink-0 text-muted-foreground" aria-hidden="true" />
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
                          aria-label={`Rewind to ${checkpoint.label}`}
                          disabled={sending || busyAction !== null}
                          onClick={() => void runRewind(checkpoint.id)}
                        >
                          <RefreshCwIcon aria-hidden="true" />
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
                    <ChevronRightIcon className="mt-1 size-3 shrink-0 text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100" aria-hidden="true" />
                  </button>
                ))}
              </div>
            )}
          </section>
        )}
      </div>
      </aside>
    </div>
  );
}
