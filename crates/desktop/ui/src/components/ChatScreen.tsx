import { useState } from "react";
import {
  CheckIcon,
  CopyIcon,
  FilePenLineIcon,
  MenuIcon,
  SquarePenIcon,
  WrenchIcon,
} from "lucide-react";

import { BrandMark } from "@/components/BrandMark";
import { Composer } from "@/components/Composer";
import { SettingsPanel } from "@/components/SettingsPanel";
import {
  Attachment,
  AttachmentContent,
  AttachmentDescription,
  AttachmentMedia,
  AttachmentTitle,
} from "@/components/ui/attachment";
import { Bubble, BubbleContent } from "@/components/ui/bubble";
import { Button } from "@/components/ui/button";
import { Marker, MarkerContent, MarkerIcon } from "@/components/ui/marker";
import { Message, MessageContent } from "@/components/ui/message";
import {
  MessageScroller,
  MessageScrollerButton,
  MessageScrollerContent,
  MessageScrollerItem,
  MessageScrollerProvider,
  MessageScrollerViewport,
} from "@/components/ui/message-scroller";
import { Spinner } from "@/components/ui/spinner";
import { providerSupportsModelPicker, type EffortId } from "@/lib/models";
import type { ChatMessage, SessionInfo, ToolPart } from "@/lib/types";
import { cn } from "@/lib/utils";

type Props = {
  session: SessionInfo;
  messages: ChatMessage[];
  draft: string;
  sending: boolean;
  model: string;
  effort: EffortId;
  onDraftChange: (value: string) => void;
  onSend: () => void;
  onNewChat: () => void;
  onChangeProvider: () => void;
  onReconnect: () => void;
  onLoadThread: (id: string) => void;
  onModelChange: (model: string) => void;
  onEffortChange: (effort: EffortId) => void;
  onResolveApproval: (approvalId: string, allow: boolean) => Promise<void>;
  /** True while a model/effort update is in flight (Rust is authoritative). */
  optionsDisabled?: boolean;
};

export function ChatScreen({
  session,
  messages,
  draft,
  sending,
  model,
  effort,
  onDraftChange,
  onSend,
  onNewChat,
  onChangeProvider,
  onReconnect,
  onLoadThread,
  onModelChange,
  onEffortChange,
  onResolveApproval,
  optionsDisabled = false,
}: Props) {
  const [settingsOpen, setSettingsOpen] = useState(false);
  const showPicker = providerSupportsModelPicker(session.provider);

  function closeSettings() {
    setSettingsOpen(false);
  }

  return (
    <section className="relative flex h-full min-h-0 flex-col overflow-hidden bg-[var(--chat-canvas)]">
      <header className="flex shrink-0 items-center justify-between border-b border-border/60 bg-[var(--chat-header)] px-4 py-2.5">
        <div className="flex items-center gap-2.5">
          <div className="grid size-7 place-items-center rounded-md bg-card ring-1 ring-border">
            <BrandMark size={18} />
          </div>
          <div className="leading-tight">
            <div className="text-sm font-semibold tracking-[-0.2px]">Zest</div>
            <div className="text-[11px] text-muted-foreground">
              {session.label} · Local
            </div>
          </div>
        </div>
        <div className="flex items-center gap-0.5">
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            title="New chat"
            onClick={onNewChat}
            disabled={sending}
          >
            <SquarePenIcon />
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            title="Settings"
            aria-expanded={settingsOpen}
            onClick={() => setSettingsOpen(true)}
          >
            <MenuIcon />
          </Button>
        </div>
      </header>

      <div className="relative min-h-0 flex-1">
        <MessageScrollerProvider>
          <MessageScroller className="absolute inset-0 pb-40">
            <MessageScrollerViewport className="scroll-fade-b">
              <MessageScrollerContent className="mx-auto w-full max-w-[var(--chat-max)] gap-6 px-4 py-6">
                {messages.length === 0 ? (
                  <MessageScrollerItem messageId="empty">
                    <div className="flex min-h-[42vh] flex-col items-center justify-center text-center">
                      <p className="max-w-[34ch] text-sm text-muted-foreground">
                        Ask about this project — Zest can read files with tools.
                      </p>
                    </div>
                  </MessageScrollerItem>
                ) : null}

                {messages.map((msg, index) => {
                  const isLast = index === messages.length - 1;
                  if (msg.role === "user") {
                    return (
                      <MessageScrollerItem
                        key={msg.id}
                        messageId={msg.id}
                        scrollAnchor={isLast}
                      >
                        <Message align="end" className="justify-end">
                          <MessageContent className="items-end">
                            <Bubble variant="secondary" align="end" className="max-w-[85%]">
                              <BubbleContent className="whitespace-pre-wrap bg-[var(--user-bubble)] text-[13.5px] leading-relaxed text-foreground">
                                {msg.text}
                              </BubbleContent>
                            </Bubble>
                          </MessageContent>
                        </Message>
                      </MessageScrollerItem>
                    );
                  }

                  return (
                    <MessageScrollerItem
                      key={msg.id}
                      messageId={msg.id}
                      scrollAnchor={isLast}
                    >
                      <Message align="start">
                        <MessageContent className="max-w-full gap-2.5">
                          <div className="text-[11px] font-medium tracking-wide text-muted-foreground/80">
                            Zest
                          </div>

                          {msg.tools.map((tool) => (
                            <ToolCard
                              key={tool.id}
                              tool={tool}
                              onResolveApproval={onResolveApproval}
                            />
                          ))}

                          {msg.thinking ? (
                            <Marker
                              role="status"
                              className="rounded-lg border border-border/70 bg-card/50 px-2.5 py-2 text-xs"
                            >
                              <MarkerIcon>
                                {msg.streaming && !msg.text ? <Spinner /> : null}
                              </MarkerIcon>
                              <MarkerContent
                                className={
                                  msg.streaming && !msg.text
                                    ? "shimmer-text whitespace-pre-wrap"
                                    : "whitespace-pre-wrap text-muted-foreground"
                                }
                              >
                                {msg.thinking}
                              </MarkerContent>
                            </Marker>
                          ) : null}

                          {msg.text ? (
                            <div className="group/assistant relative">
                              <div className="whitespace-pre-wrap text-[15px] leading-[1.65] text-foreground">
                                {msg.text}
                                {msg.streaming ? (
                                  <span className="ml-0.5 inline-block h-4 w-1.5 animate-pulse bg-primary align-text-bottom" />
                                ) : null}
                              </div>
                              {!msg.streaming ? (
                                <div className="mt-2 flex items-center gap-0.5 opacity-0 transition-opacity group-hover/assistant:opacity-100 focus-within:opacity-100">
                                  <CopyButton text={msg.text} />
                                </div>
                              ) : null}
                            </div>
                          ) : null}

                          {msg.error ? (
                            <Bubble variant="destructive" align="start">
                              <BubbleContent>{msg.error}</BubbleContent>
                            </Bubble>
                          ) : null}

                          {msg.streaming &&
                          !msg.text &&
                          !msg.thinking &&
                          msg.tools.length === 0 ? (
                            <Marker role="status">
                              <MarkerIcon>
                                <Spinner />
                              </MarkerIcon>
                              <MarkerContent className="shimmer-text">Thinking…</MarkerContent>
                            </Marker>
                          ) : null}

                          {msg.streaming &&
                          !msg.text &&
                          msg.tools.length > 0 &&
                          !msg.tools.some(
                            (t) =>
                              t.status === "running" || t.status === "awaiting_approval"
                          ) ? (
                            <Marker role="status">
                              <MarkerIcon>
                                <Spinner />
                              </MarkerIcon>
                              <MarkerContent className="shimmer-text">Working…</MarkerContent>
                            </Marker>
                          ) : null}
                        </MessageContent>
                      </Message>
                    </MessageScrollerItem>
                  );
                })}
              </MessageScrollerContent>
            </MessageScrollerViewport>
            <MessageScrollerButton />
          </MessageScroller>
        </MessageScrollerProvider>

        <Composer
          value={draft}
          model={model}
          effort={effort}
          meta={`${session.label} · Local`}
          sending={sending}
          showModelPicker={showPicker}
          optionsDisabled={optionsDisabled}
          onChange={onDraftChange}
          onSubmit={onSend}
          onModelChange={onModelChange}
          onEffortChange={onEffortChange}
        />
      </div>

      <SettingsPanel
        open={settingsOpen}
        session={session}
        model={model}
        effort={effort}
        sending={sending}
        onClose={closeSettings}
        onNewChat={() => {
          closeSettings();
          onNewChat();
        }}
        onChangeProvider={() => {
          closeSettings();
          onChangeProvider();
        }}
        onReconnect={() => {
          closeSettings();
          onReconnect();
        }}
        onLoadThread={(id) => {
          closeSettings();
          onLoadThread(id);
        }}
      />
    </section>
  );
}

function ToolCard({
  tool,
  onResolveApproval,
}: {
  tool: ToolPart;
  onResolveApproval: (approvalId: string, allow: boolean) => Promise<void>;
}) {
  const awaiting = tool.status === "awaiting_approval";
  const [busy, setBusy] = useState<"allow" | "deny" | null>(null);

  async function resolve(allow: boolean) {
    if (!tool.approvalId || busy !== null) return;
    setBusy(allow ? "allow" : "deny");
    try {
      await onResolveApproval(tool.approvalId, allow);
    } catch {
      // App restores the approval card; clear local busy so Allow/Deny work again.
      setBusy(null);
    }
  }

  if (awaiting) {
    return (
      <div className="w-full max-w-full overflow-hidden rounded-xl border border-border/80 bg-card/90">
        <div className="flex items-start gap-2.5 border-b border-border/60 px-3 py-2.5">
          <div className="mt-0.5 grid size-8 place-items-center rounded-lg bg-muted text-foreground">
            <FilePenLineIcon className="size-4" />
          </div>
          <div className="min-w-0 flex-1">
            <div className="text-xs font-medium text-foreground">
              Allow {tool.name}?
            </div>
            <div className="mt-0.5 font-mono text-[11px] text-muted-foreground">
              {tool.path || tool.summary || "Write to project file"}
            </div>
          </div>
        </div>
        {tool.diff ? (
          <pre className="max-h-48 overflow-auto border-b border-border/60 bg-[var(--chat-canvas)] px-3 py-2 font-mono text-[11px] leading-relaxed text-muted-foreground whitespace-pre-wrap">
            {tool.diff}
          </pre>
        ) : null}
        <div className="flex items-center justify-end gap-2 px-3 py-2">
          <Button
            type="button"
            variant="ghost"
            size="sm"
            disabled={busy !== null}
            onClick={() => {
              void resolve(false);
            }}
          >
            {busy === "deny" ? "Denying…" : "Deny"}
          </Button>
          <Button
            type="button"
            size="sm"
            disabled={busy !== null}
            onClick={() => {
              void resolve(true);
            }}
          >
            {busy === "allow" ? "Allowing…" : "Allow once"}
          </Button>
        </div>
      </div>
    );
  }

  return (
    <Attachment
      state={
        tool.status === "running"
          ? "processing"
          : tool.status === "error"
            ? "error"
            : "done"
      }
      size="sm"
      className="max-w-full border-border/80 bg-card/80"
    >
      <AttachmentMedia>
        {tool.status === "running" ? <Spinner /> : <WrenchIcon />}
      </AttachmentMedia>
      <AttachmentContent>
        <AttachmentTitle
          className={tool.status === "running" ? "shimmer-text" : undefined}
        >
          {tool.name}
        </AttachmentTitle>
        {tool.summary ? (
          <AttachmentDescription className="line-clamp-2 font-mono text-[11px]">
            {tool.summary}
          </AttachmentDescription>
        ) : null}
      </AttachmentContent>
    </Attachment>
  );
}

function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);

  return (
    <Button
      type="button"
      variant="ghost"
      size="icon-sm"
      className={cn("text-muted-foreground hover:text-foreground")}
      title="Copy"
      onClick={async () => {
        try {
          await navigator.clipboard.writeText(text);
          setCopied(true);
          window.setTimeout(() => setCopied(false), 1200);
        } catch {
          /* ignore */
        }
      }}
    >
      {copied ? <CheckIcon className="size-3.5 text-[var(--success)]" /> : <CopyIcon className="size-3.5" />}
    </Button>
  );
}
