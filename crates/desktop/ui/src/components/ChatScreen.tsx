import { useState } from "react";
import {
  CheckIcon,
  CopyIcon,
  MenuIcon,
  SquarePenIcon,
} from "lucide-react";

import {
  ChatHistorySidebar,
  readSidebarOpen,
  writeSidebarOpen,
} from "@/components/ChatHistorySidebar";
import { CommandOutputCard } from "@/components/CommandOutputCard";
import { Composer } from "@/components/Composer";
import { DiffViewer, type DiffViewerTarget } from "@/components/DiffViewer";
import { Markdown } from "@/components/Markdown";
import { SettingsPanel } from "@/components/SettingsPanel";
import { ToolCallRow } from "@/components/ToolCallRow";
import { ToolRunGroup } from "@/components/ToolRunGroup";
import { UserAvatarButton } from "@/components/UserAvatarButton";
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
import { ZestPulse } from "@/components/ZestPulse";
import { LinkifyText } from "@/lib/linkify";
import { sessionSupportsModelPicker, type EffortId } from "@/lib/models";
import { groupToolRuns } from "@/lib/toolRuns";
import type {
  ApprovalChoice,
  ApprovalMode,
  ChatMessage,
  PreparedAttachment,
  SessionInfo,
  UserProfile,
} from "@/lib/types";
import { cn } from "@/lib/utils";

function shortRoot(root: string): string {
  const cleaned = root.replace(/^\\\\\?\\UNC\\/i, "\\\\").replace(/^\\\\\?\\/, "");
  const normalized = cleaned.replace(/\\/g, "/");
  const parts = normalized.split("/").filter(Boolean);
  if (parts.length <= 2) return cleaned;
  return parts.slice(-2).join("/");
}

type Props = {
  session: SessionInfo;
  messages: ChatMessage[];
  draft: string;
  attachments: PreparedAttachment[];
  branch: string | null;
  profile: UserProfile;
  sending: boolean;
  model: string;
  effort: EffortId;
  onDraftChange: (value: string) => void;
  onSend: () => void;
  onStop?: () => void;
  onNewChat: () => void;
  onDeleteThread: (id: string, projectPath: string) => Promise<void>;
  onOpenProjectChat: (options: {
    root: string;
    threadId?: string;
    newThread?: boolean;
  }) => Promise<void>;
  onChangeProvider: () => void;
  onReloadSession?: () => Promise<void>;
  /** Re-run sign-in for a provider whose credentials the gateway rejected. */
  onReconnectProvider?: (providerId: string) => void;
  onReconnect: () => void;
  onLoadThread: (id: string) => void;
  onModelChange: (model: string) => void;
  onEffortChange: (effort: EffortId) => void;
  approvalMode: ApprovalMode;
  onApprovalModeChange: (mode: ApprovalMode) => void;
  onResolveApproval: (
    approvalId: string,
    decision: ApprovalChoice
  ) => Promise<void>;
  onAttachFiles: () => void;
  onOpenFolder: () => void;
  onRemoveAttachment: (id: string) => void;
  onPasteImages: (files: File[]) => void;
  onProfileChange: (profile: UserProfile) => void;
  optionsDisabled?: boolean;
};

export function ChatScreen({
  session,
  messages,
  draft,
  attachments,
  branch,
  profile,
  sending,
  model,
  effort,
  onDraftChange,
  onSend,
  onStop,
  onNewChat,
  onDeleteThread,
  onOpenProjectChat,
  onChangeProvider,
  onReloadSession,
  onReconnectProvider,
  onReconnect,
  onLoadThread,
  onModelChange,
  onEffortChange,
  approvalMode,
  onApprovalModeChange,
  onResolveApproval,
  onAttachFiles,
  onOpenFolder,
  onRemoveAttachment,
  onPasteImages,
  onProfileChange,
  optionsDisabled = false,
}: Props) {
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [focusUser, setFocusUser] = useState(false);
  const [sidebarOpen, setSidebarOpen] = useState(readSidebarOpen);
  const [diffTarget, setDiffTarget] = useState<DiffViewerTarget | null>(null);
  const showPicker = sessionSupportsModelPicker(session.models);
  const folderLabel = shortRoot(session.root);

  function closeSettings() {
    setSettingsOpen(false);
    setFocusUser(false);
  }

  function setSidebar(next: boolean) {
    setSidebarOpen(next);
    writeSidebarOpen(next);
  }

  return (
    <section className="relative flex h-full min-h-0 overflow-hidden bg-[var(--chat-canvas)]">
      <ChatHistorySidebar
        open={sidebarOpen}
        activeThreadId={session.threadId}
        activeProjectPath={session.root}
        sending={sending}
        onOpenChange={setSidebar}
        onNewChat={onNewChat}
        onLoadThread={onLoadThread}
        onOpenProjectChat={onOpenProjectChat}
        onDeleteThread={onDeleteThread}
        onOpenFolder={onOpenFolder}
      />

      <div className="flex min-h-0 min-w-0 flex-1 flex-col">
        <header className="flex shrink-0 items-center justify-between border-b border-border/60 bg-[var(--chat-header)] px-4 py-2.5">
          <div className="flex items-center gap-2.5">
            <UserAvatarButton
              avatarDataUrl={profile.avatarDataUrl}
              displayName={profile.displayName}
              onClick={() => {
                setFocusUser(true);
                setSettingsOpen(true);
              }}
            />
            <div className="leading-tight">
              <div className="text-sm font-semibold tracking-[-0.2px]">
                {profile.displayName.trim() || "Zest"}
              </div>
              <div
                className="max-w-[48ch] truncate text-[11px] text-muted-foreground"
                title={`${session.root}${branch ? ` · ${branch}` : ""}`}
              >
                {session.label} · {folderLabel}
                {branch ? ` · ${branch}` : ""}
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
              onClick={() => {
                setFocusUser(false);
                setSettingsOpen(true);
              }}
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
                          Ask about this project — paste images, attach PDFs, or open
                          another folder from +.
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
                                  <LinkifyText text={msg.text} />
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

                            {msg.tools.length > 0 ? (
                              <div className="flex w-full max-w-full flex-col gap-0.5">
                                {groupToolRuns(msg.tools).map((run) =>
                                  run.kind === "group" ? (
                                    <ToolRunGroup
                                      key={`group-${run.tools[0].id}`}
                                      tools={run.tools}
                                      summary={run.summary}
                                      onResolveApproval={onResolveApproval}
                                      onOpenDiff={(path, diff) =>
                                        setDiffTarget({ path, diff })
                                      }
                                    />
                                  ) : (
                                    <ToolCallRow
                                      key={run.tool.id}
                                      tool={run.tool}
                                      onResolveApproval={onResolveApproval}
                                      onOpenDiff={(path, diff) =>
                                        setDiffTarget({ path, diff })
                                      }
                                    />
                                  )
                                )}
                              </div>
                            ) : null}

                            {msg.thinking ? (
                              <Marker
                                role="status"
                                className="items-start gap-2 border-0 bg-transparent px-0 py-0.5 text-xs text-[#8a8f98]"
                              >
                                {msg.streaming && !msg.text ? (
                                  <MarkerIcon className="mt-0.5">
                                    <ZestPulse size={14} />
                                  </MarkerIcon>
                                ) : null}
                                <MarkerContent
                                  className={cn(
                                    "min-w-0 text-[#8a8f98]",
                                    msg.streaming && !msg.text && "shimmer-text"
                                  )}
                                >
                                  <Markdown className="text-xs text-[#8a8f98] [&_a]:text-[#6b86d4] [&_p]:mb-1.5 [&_p]:leading-relaxed [&_p]:text-[#8a8f98] [&_strong]:font-medium [&_strong]:text-[#9aa0a8]">
                                    {msg.thinking}
                                  </Markdown>
                                </MarkerContent>
                              </Marker>
                            ) : null}

                            {msg.text ? (
                              // The answer to a slash command reads as a
                              // document, not a chat reply — frame it as one.
                              // Tool rows stay outside the card: they are how
                              // the answer was reached, not part of it.
                              msg.command ? (
                                <CommandOutputCard
                                  command={msg.command}
                                  text={msg.text}
                                  streaming={msg.streaming}
                                >
                                  <Markdown>{msg.text}</Markdown>
                                  {msg.streaming ? (
                                    <span className="ml-1.5 inline-flex items-center gap-1.5 align-middle">
                                      <ZestPulse size={12} />
                                      <span className="inline-block h-4 w-1.5 animate-pulse bg-foreground/70" />
                                    </span>
                                  ) : null}
                                </CommandOutputCard>
                              ) : (
                                <div className="group/assistant relative">
                                  <div className="relative">
                                    <Markdown>{msg.text}</Markdown>
                                    {msg.streaming ? (
                                      <span className="ml-1.5 inline-flex items-center gap-1.5 align-middle">
                                        <ZestPulse size={12} />
                                        <span className="inline-block h-4 w-1.5 animate-pulse bg-foreground/70" />
                                      </span>
                                    ) : null}
                                  </div>
                                  {!msg.streaming ? (
                                    <div className="mt-2 flex items-center gap-0.5 opacity-0 transition-opacity group-hover/assistant:opacity-100 focus-within:opacity-100">
                                      <CopyButton text={msg.text} />
                                    </div>
                                  ) : null}
                                </div>
                              )
                            ) : null}

                            {msg.error ? (
                              <Bubble variant="destructive" align="start">
                                <BubbleContent>
                                  {msg.error}
                                  {/* Only auth failures get this — signing in
                                      again fixes nothing else, and the picker's
                                      Reconnect is unreachable from here. */}
                                  {msg.reconnectProvider && onReconnectProvider ? (
                                    <div className="mt-2.5">
                                      <Button
                                        type="button"
                                        size="sm"
                                        variant="outline"
                                        onClick={() =>
                                          onReconnectProvider(
                                            msg.reconnectProvider as string
                                          )
                                        }
                                      >
                                        Reconnect {msg.reconnectProvider}
                                      </Button>
                                    </div>
                                  ) : null}
                                </BubbleContent>
                              </Bubble>
                            ) : null}

                            {msg.streaming &&
                            !msg.text &&
                            !msg.thinking &&
                            msg.tools.length === 0 ? (
                              <Marker role="status">
                                <MarkerIcon>
                                  <ZestPulse size={14} />
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
                                  <ZestPulse size={14} />
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
            approvalMode={approvalMode}
            onApprovalModeChange={onApprovalModeChange}
            value={draft}
            model={model}
            effort={effort}
            models={session.models}
            defaultModel={session.defaultModel}
            folderLabel={folderLabel}
            branch={branch}
            contextRefreshKey={`${session.threadId}:${messages.length}:${sending ? 1 : 0}`}
            sending={sending}
            showModelPicker={showPicker}
            optionsDisabled={optionsDisabled}
            attachments={attachments}
            onChange={onDraftChange}
            onSubmit={onSend}
            onStop={onStop}
            onModelChange={onModelChange}
            onEffortChange={onEffortChange}
            onAttachFiles={onAttachFiles}
            onOpenFolder={onOpenFolder}
            onRemoveAttachment={onRemoveAttachment}
            onPasteImages={onPasteImages}
          />
        </div>
      </div>

      <SettingsPanel
        open={settingsOpen}
        session={session}
        model={model}
        effort={effort}
        sending={sending}
        profile={profile}
        focusUser={focusUser}
        onClose={closeSettings}
        onChangeProvider={() => {
          closeSettings();
          onChangeProvider();
        }}
        onReloadSession={onReloadSession}
        onReconnect={() => {
          closeSettings();
          onReconnect();
        }}
        onOpenFolder={() => {
          closeSettings();
          onOpenFolder();
        }}
        onProfileChange={onProfileChange}
      />

      <DiffViewer target={diffTarget} onClose={() => setDiffTarget(null)} />
    </section>
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
