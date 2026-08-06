import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  FileIcon,
  FileTextIcon,
  FolderOpenIcon,
  ImageIcon,
  CommandIcon,
  PanelRightOpenIcon,
  SettingsIcon,
  TriangleAlertIcon,
  XIcon,
} from "lucide-react";

import {
  ChatHistorySidebar,
  readSidebarOpen,
  writeSidebarOpen,
} from "@/components/ChatHistorySidebar";
import { CommandOutputCard } from "@/components/CommandOutputCard";
import { CommandPalette, type PaletteAction } from "@/components/CommandPalette";
import { Composer } from "@/components/Composer";
import { DiffViewer, type DiffViewerTarget } from "@/components/DiffViewer";
import { MarkdownActions } from "@/components/MarkdownActions";
import { PlanningQuestionnaire } from "@/components/PlanningQuestionnaire";
import { looksLikeDocument } from "@/lib/documentShape";
import { buildablePlanId } from "@/lib/planActions";
import { planningQuestionFor } from "@/lib/planningQuestion";
import { Markdown } from "@/components/Markdown";
import { ProviderSwitchSheet } from "@/components/ProviderSwitchSheet";
import { SettingsPanel } from "@/components/SettingsPanel";
import { ToolCallRow } from "@/components/ToolCallRow";
import { ToolRunGroup } from "@/components/ToolRunGroup";
import { UserAvatarButton } from "@/components/UserAvatarButton";
import { WorkbenchPanel } from "@/components/WorkbenchPanel";
import {
  Attachment,
  AttachmentContent,
  AttachmentGroup,
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
import { ZestPulse } from "@/components/ZestPulse";
import { LinkifyText } from "@/lib/linkify";
import { sessionSupportsModelPicker, type EffortId } from "@/lib/models";
import { groupToolRuns } from "@/lib/toolRuns";
import { useKeybindings } from "@/lib/useKeybindings";
import type {
  ApprovalChoice,
  ApprovalMode,
  ChatMessage,
  PreparedAttachment,
  ProviderRow,
  SessionInfo,
  SessionWarning,
  UserProfile,
  WorkspaceReview,
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
  onSend: (text?: string) => void;
  onStop?: () => void;
  onNewChat: () => void;
  onForkThread: () => Promise<void>;
  onRewindThread: (checkpointId: string) => Promise<void>;
  workspaceReview: WorkspaceReview | null;
  onVerifyWorkspace: () => Promise<void>;
  compacting?: boolean;
  onDeleteThread: (id: string, projectPath: string) => Promise<void>;
  onOpenProjectChat: (options: {
    root: string;
    threadId?: string;
    newThread?: boolean;
  }) => Promise<void>;
  providers: ProviderRow[];
  onSwitchProvider: (providerId: string) => Promise<void>;
  onReloadSession?: () => Promise<void>;
  /** Re-run sign-in for a provider whose credentials the gateway rejected. */
  onReconnectProvider?: (providerId: string) => void;
  onRefreshProviders: () => Promise<void>;
  onReconnect: () => void;
  /** A background verification that failed after this chat opened. */
  sessionWarning?: SessionWarning | null;
  onDismissWarning?: () => void;
  onLoadThread: (id: string) => void;
  onModelChange: (model: string) => void;
  onEffortChange: (effort: EffortId) => void;
  approvalMode: ApprovalMode;
  onApprovalModeChange: (mode: ApprovalMode) => void;
  /** Leave Plan mode and build the newest plan. */
  onBuildPlan?: () => void;
  /** Show the profile screen (avatar click). */
  onOpenProfile?: () => void;
  /**
   * Bumped to request the User section of Settings — the profile screen sends
   * edits here rather than duplicating the form.
   */
  settingsRequest?: number;
  onResolveApproval: (
    approvalId: string,
    decision: ApprovalChoice
  ) => Promise<void>;
  onResolveQuestion: (questionId: string, answer: string) => Promise<void>;
  onAttachFiles: () => void;
  onOpenFolder: () => void;
  onRemoveAttachment: (id: string) => void;
  onPasteImages: (files: File[]) => void;
  onProfileChange: (profile: UserProfile) => void;
  optionsDisabled?: boolean;
};

function focusComposer() {
  const el = document.getElementById(
    "zest-composer-input"
  ) as HTMLTextAreaElement | null;
  if (!el) return;
  el.focus();
  const len = el.value.length;
  el.setSelectionRange(len, len);
}

type ChatMessageRowProps = {
  message: ChatMessage;
  isLast: boolean;
  sending: boolean;
  approvalMode: ApprovalMode;
  planToBuild: string | null;
  onBuildPlan?: () => void;
  onResolveApproval: (
    approvalId: string,
    decision: ApprovalChoice
  ) => Promise<void>;
  onOpenDiff: (path: string, diff: string) => void;
  onReconnectProvider?: (providerId: string) => void;
  onSend: (text?: string) => void;
  onResolveQuestion: (questionId: string, answer: string) => Promise<void>;
};

/**
 * Keep settled messages out of the streaming render path. Appending a delta
 * still updates the active row, but unchanged rows now retain their existing
 * React subtree instead of rebuilding every tool card and Markdown document.
 */
const ChatMessageRow = memo(function ChatMessageRow({
  message: msg,
  isLast,
  sending,
  approvalMode,
  planToBuild,
  onBuildPlan,
  onResolveApproval,
  onOpenDiff,
  onReconnectProvider,
  onSend,
  onResolveQuestion,
}: ChatMessageRowProps) {
  if (msg.role === "user") {
    return (
      <MessageScrollerItem id={`message-${msg.id}`} messageId={msg.id} scrollAnchor={isLast}>
        <Message align="end" className="justify-end">
          <MessageContent className="items-end gap-1.5">
            {msg.attachments && msg.attachments.length > 0 ? (
              <AttachmentGroup className="justify-end">
                {msg.attachments.map((att) => (
                  <Attachment key={`${msg.id}-${att.name}`} size="sm">
                    <AttachmentMedia variant="icon">
                      {att.kind === "pdf" ? (
                        <FileTextIcon />
                      ) : att.kind === "image" ? (
                        <ImageIcon />
                      ) : (
                        <FileIcon />
                      )}
                    </AttachmentMedia>
                    <AttachmentContent>
                      <AttachmentTitle>{att.name}</AttachmentTitle>
                    </AttachmentContent>
                  </Attachment>
                ))}
              </AttachmentGroup>
            ) : null}
            {msg.text.trim() ? (
              <Bubble variant="secondary" align="end" className="max-w-[85%]">
                <BubbleContent className="whitespace-pre-wrap bg-[var(--user-bubble)] text-[13.5px] leading-relaxed text-foreground">
                  <LinkifyText text={msg.text} />
                </BubbleContent>
              </Bubble>
            ) : null}
          </MessageContent>
        </Message>
      </MessageScrollerItem>
    );
  }

  const structuredQuestion = isLast ? msg.question : undefined;
  const planningQuestion =
    structuredQuestion ?? (isLast ? planningQuestionFor(msg) : null);
  const submitQuestion = structuredQuestion?.questionId
    ? (answer: string) =>
        onResolveQuestion(structuredQuestion.questionId as string, answer)
    : onSend;

  return (
    <MessageScrollerItem id={`message-${msg.id}`} messageId={msg.id} scrollAnchor={isLast}>
      <Message align="start">
        <MessageContent className="w-full max-w-full gap-2.5">
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
                    onOpenDiff={onOpenDiff}
                  />
                ) : (
                  <ToolCallRow
                    key={run.tool.id}
                    tool={run.tool}
                    onResolveApproval={onResolveApproval}
                    onOpenDiff={onOpenDiff}
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
                <Markdown
                  streaming={msg.streaming}
                  className="text-xs text-[#8a8f98] [&_a]:text-[#6b86d4] [&_p]:mb-1.5 [&_p]:leading-relaxed [&_p]:text-[#8a8f98] [&_strong]:font-medium [&_strong]:text-[#9aa0a8]"
                >
                  {msg.thinking}
                </Markdown>
              </MarkerContent>
            </Marker>
          ) : null}

          {planningQuestion ? (
            <PlanningQuestionnaire
              question={planningQuestion}
              disabled={structuredQuestion ? false : sending}
              onSubmit={submitQuestion}
            />
          ) : msg.text ? (
            msg.command && looksLikeDocument(msg.text) ? (
              <CommandOutputCard
                command={msg.command}
                text={msg.text}
                streaming={msg.streaming}
                action={
                  msg.id === planToBuild && onBuildPlan
                    ? {
                        label: "Build plan",
                        hint:
                          approvalMode === "plan"
                            ? "Leaves Plan mode so the steps can run"
                            : undefined,
                        disabled: sending,
                        onClick: onBuildPlan,
                      }
                    : undefined
                }
              >
                <Markdown streaming={msg.streaming}>{msg.text}</Markdown>
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
                  <Markdown streaming={msg.streaming}>{msg.text}</Markdown>
                  {msg.streaming ? (
                    <span className="ml-1.5 inline-flex items-center gap-1.5 align-middle">
                      <ZestPulse size={12} />
                      <span className="inline-block h-4 w-1.5 animate-pulse bg-foreground/70" />
                    </span>
                  ) : null}
                </div>
                {!msg.streaming ? (
                  <div className="mt-2 flex items-center gap-0.5 text-muted-foreground opacity-70 transition-opacity hover:opacity-100 focus-within:opacity-100">
                    <MarkdownActions text={msg.text} />
                  </div>
                ) : null}
              </div>
            )
          ) : null}

          {msg.error ? (
            <Bubble variant="destructive" align="start">
              <BubbleContent>
                {msg.error}
                {msg.reconnectProvider && onReconnectProvider ? (
                  <div className="mt-2.5">
                    <Button
                      type="button"
                      size="sm"
                      variant="outline"
                      onClick={() =>
                        onReconnectProvider(msg.reconnectProvider as string)
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
              <MarkerContent className="shimmer-text">Thinking...</MarkerContent>
            </Marker>
          ) : null}

          {msg.streaming &&
          !msg.text &&
          msg.tools.length > 0 &&
          !msg.tools.some(
            (tool) =>
              tool.status === "running" || tool.status === "awaiting_approval"
          ) ? (
            <Marker role="status">
              <MarkerIcon>
                <ZestPulse size={14} />
              </MarkerIcon>
              <MarkerContent className="shimmer-text">Working...</MarkerContent>
            </Marker>
          ) : null}
        </MessageContent>
      </Message>
    </MessageScrollerItem>
  );
});

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
  onForkThread,
  onRewindThread,
  workspaceReview,
  onVerifyWorkspace,
  compacting = false,
  onDeleteThread,
  onOpenProjectChat,
  providers,
  onSwitchProvider,
  onReloadSession,
  onReconnectProvider,
  onRefreshProviders,
  onReconnect,
  sessionWarning = null,
  onDismissWarning,
  onLoadThread,
  onModelChange,
  onEffortChange,
  approvalMode,
  onApprovalModeChange,
  onBuildPlan,
  onResolveApproval,
  onResolveQuestion,
  onAttachFiles,
  onOpenFolder,
  onRemoveAttachment,
  onPasteImages,
  onProfileChange,
  onOpenProfile,
  settingsRequest = 0,
  optionsDisabled = false,
}: Props) {
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [focusUser, setFocusUser] = useState(false);
  /** Bumped to open Settings with the Keyboard shortcuts section expanded. */
  const [shortcutsRequest, setShortcutsRequest] = useState(0);
  const [sidebarOpen, setSidebarOpen] = useState(readSidebarOpen);
  const [diffTarget, setDiffTarget] = useState<DiffViewerTarget | null>(null);
  const [providerSwitchOpen, setProviderSwitchOpen] = useState(false);
  const [providerSwitchBusy, setProviderSwitchBusy] = useState(false);
  const [workbenchOpen, setWorkbenchOpen] = useState(false);
  const [workbenchAutoOpened, setWorkbenchAutoOpened] = useState(false);
  const lastAutoWorkbenchKey = useRef("");
  const [paletteOpen, setPaletteOpen] = useState(false);
  const closeWorkbench = useCallback(() => {
    setWorkbenchOpen(false);
    setWorkbenchAutoOpened(false);
    requestAnimationFrame(() => {
      document.getElementById("workbench-toggle")?.focus();
    });
  }, []);
  const openDiff = useCallback(
    (path: string, diff: string) => setDiffTarget({ path, diff }),
    []
  );
  const showPicker = sessionSupportsModelPicker(session.models);
  const folderLabel = shortRoot(session.root);
  const activeWorkbenchKey = useMemo(() => {
    const activeTools = messages.flatMap((message) =>
      message.role === "assistant"
        ? message.tools
            .filter((tool) => tool.status === "running" || tool.status === "awaiting_approval")
            .map((tool) => `${tool.id}:${tool.status}`)
        : []
    );
    return [compacting ? "compacting" : "", ...activeTools].filter(Boolean).join("|");
  }, [compacting, messages]);
  const planToBuild = useMemo(() => buildablePlanId(messages), [messages]);

  useEffect(() => {
    if (!activeWorkbenchKey) {
      lastAutoWorkbenchKey.current = "";
      return;
    }
    if (lastAutoWorkbenchKey.current === activeWorkbenchKey) return;
    lastAutoWorkbenchKey.current = activeWorkbenchKey;
    setWorkbenchAutoOpened(true);
    setWorkbenchOpen(true);
  }, [activeWorkbenchKey]);

  const jumpToMessage = useCallback((messageId: string) => {
    document.getElementById(`message-${messageId}`)?.scrollIntoView({
      behavior: "smooth",
      block: "center",
    });
  }, []);

  const paletteActions = useMemo<PaletteAction[]>(
    () => [
      {
        id: "new-chat",
        label: "New chat",
        description: "Start a fresh conversation in this project",
        shortcut: "Ctrl+N",
        run: () => {
          if (!sending) onNewChat();
        },
      },
      {
        id: "toggle-workbench",
        label: workbenchOpen ? "Close workbench" : "Open workbench",
        description: "Inspect activity, outline, and recovery checkpoints",
        run: () => {
          setWorkbenchAutoOpened(false);
          setWorkbenchOpen((value) => !value);
        },
      },
      {
        id: "open-provider",
        label: "Switch provider",
        description: "Choose a configured model provider",
        shortcut: "Ctrl+Shift+M",
        run: () => setProviderSwitchOpen(true),
      },
      {
        id: "open-settings",
        label: "Open settings",
        description: "Configure Zest and keyboard shortcuts",
        shortcut: "Ctrl+,",
        run: () => {
          setFocusUser(false);
          setSettingsOpen(true);
        },
      },
    ],
    [onNewChat, sending, workbenchOpen]
  );

  // A bump means "open the User section". Zero is the initial value, so the
  // panel does not fly open on mount.
  useEffect(() => {
    if (settingsRequest <= 0) return;
    setFocusUser(true);
    setSettingsOpen(true);
  }, [settingsRequest]);

  function closeSettings() {
    setSettingsOpen(false);
    setFocusUser(false);
  }

  function setSidebar(next: boolean) {
    setSidebarOpen(next);
    writeSidebarOpen(next);
  }

  // Escape stays hand-written and is not rebindable: it means "dismiss what is
  // on top", so it has to read the stack of open surfaces in order.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      if (diffTarget) {
        e.preventDefault();
        setDiffTarget(null);
        return;
      }
      if (providerSwitchOpen) {
        e.preventDefault();
        if (!providerSwitchBusy) setProviderSwitchOpen(false);
        return;
      }
      if (settingsOpen) {
        e.preventDefault();
        closeSettings();
        return;
      }
      if (paletteOpen) {
        e.preventDefault();
        setPaletteOpen(false);
        return;
      }
      if (sending && onStop) {
        e.preventDefault();
        onStop();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [diffTarget, onStop, paletteOpen, providerSwitchBusy, providerSwitchOpen, sending, settingsOpen]);

  // Everything else comes from the registry, so the shortcuts editor is the one
  // place that decides which key runs which command.
  useKeybindings({
    "chat.new": () => {
      if (!sending) onNewChat();
    },
    "chat.stop": () => {
      if (sending) onStop?.();
    },
    "focus.composer": focusComposer,
    "view.sidebar": () => setSidebar(!sidebarOpen),
    "view.settings": () => {
      setFocusUser(false);
      setSettingsOpen(true);
    },
    "view.shortcuts": () => {
      setFocusUser(false);
      setShortcutsRequest((n) => n + 1);
      setSettingsOpen(true);
    },
    "view.profile": () => onOpenProfile?.(),
    "view.provider": () => setProviderSwitchOpen(true),
    "view.palette": () => setPaletteOpen(true),
  });

  return (
    <section className="relative flex h-full min-h-0 overflow-hidden bg-[var(--chat-canvas)]">
      <ChatHistorySidebar
        open={sidebarOpen}
        activeThreadId={session.threadId}
        activeProjectPath={session.root}
        activeProviderId={session.provider}
        sending={sending}
        onOpenChange={setSidebar}
        onNewChat={onNewChat}
        onLoadThread={onLoadThread}
        onSwitchProvider={onSwitchProvider}
        onOpenProjectChat={onOpenProjectChat}
        onForkThread={onForkThread}
        onDeleteThread={onDeleteThread}
        onOpenFolder={onOpenFolder}
      />

      <div className="flex min-h-0 min-w-0 flex-1 flex-col">
        <header className="flex shrink-0 items-center justify-between border-b border-border/60 bg-[var(--chat-header)] px-4 py-2.5">
          <div className="flex items-center gap-2.5">
            <UserAvatarButton
              avatarDataUrl={profile.avatarDataUrl}
              displayName={profile.displayName}
              title="Your profile"
              onClick={() => {
                if (onOpenProfile) {
                  onOpenProfile();
                  return;
                }
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
              title="Command palette (Ctrl+K)"
              aria-label="Open command palette"
              aria-expanded={paletteOpen}
              onClick={() => setPaletteOpen(true)}
            >
              <CommandIcon />
            </Button>
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              title={workbenchOpen ? "Close Workbench" : "Open Workbench"}
              aria-label={workbenchOpen ? "Close Workbench" : "Open Workbench"}
              aria-controls="workbench-panel"
              aria-expanded={workbenchOpen}
              id="workbench-toggle"
              onClick={() => {
                setWorkbenchAutoOpened(false);
                setWorkbenchOpen((value) => !value);
              }}
            >
              <PanelRightOpenIcon aria-hidden="true" />
            </Button>
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              title="Settings (Ctrl+,)"
              aria-expanded={settingsOpen}
              onClick={() => {
                setFocusUser(false);
                setSettingsOpen(true);
              }}
            >
              <SettingsIcon />
            </Button>
          </div>
        </header>

        {sessionWarning ? (
          <div
            role="status"
            className="flex items-start gap-2.5 border-b border-amber-500/30 bg-amber-500/10 px-4 py-2.5 animate-in fade-in slide-in-from-top-1 duration-200"
          >
            <TriangleAlertIcon className="mt-px size-3.5 shrink-0 text-amber-400" aria-hidden />
            <p className="m-0 min-w-0 flex-1 whitespace-pre-wrap text-[11px] leading-relaxed text-amber-200/90">
              {sessionWarning.message}
            </p>
            {sessionWarning.offerReconnect ? (
              <Button type="button" size="sm" variant="outline" onClick={onReconnect}>
                Reconnect
              </Button>
            ) : null}
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              title="Dismiss"
              aria-label="Dismiss this warning"
              onClick={onDismissWarning}
            >
              <XIcon />
            </Button>
          </div>
        ) : null}

        <div className="relative min-h-0 flex-1">
          <MessageScrollerProvider autoScroll scrollEdgeThreshold={24}>
            <MessageScroller className="absolute inset-0 pb-40">
              <MessageScrollerViewport className="scroll-fade-b">
                <MessageScrollerContent className="mx-auto w-full max-w-[var(--chat-max)] gap-6 px-4 py-6">
                  {messages.length === 0 ? (
                    <MessageScrollerItem messageId="empty">
                      <div className="flex min-h-[42vh] flex-col items-center justify-center gap-4 text-center">
                        <p className="max-w-[34ch] text-sm text-muted-foreground">
                          Ask about this project — paste images, attach files, or
                          open another folder from +.
                        </p>
                        <div className="flex flex-wrap items-center justify-center gap-2">
                          <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={() => {
                              onDraftChange("/plan ");
                              requestAnimationFrame(() => focusComposer());
                            }}
                          >
                            Plan this repo
                          </Button>
                          <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={() => {
                              onDraftChange("/");
                              requestAnimationFrame(() => focusComposer());
                            }}
                          >
                            / commands
                          </Button>
                          <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={onOpenFolder}
                          >
                            <FolderOpenIcon className="size-3.5" />
                            Open folder
                          </Button>
                        </div>
                      </div>
                    </MessageScrollerItem>
                  ) : null}

                  {messages.map((msg, index) => (
                    <ChatMessageRow
                      key={msg.id}
                      message={msg}
                      isLast={index === messages.length - 1}
                      sending={sending}
                      approvalMode={approvalMode}
                      planToBuild={planToBuild}
                      onBuildPlan={onBuildPlan}
                      onResolveApproval={onResolveApproval}
                      onOpenDiff={openDiff}
                      onReconnectProvider={onReconnectProvider}
                      onSend={onSend}
                      onResolveQuestion={onResolveQuestion}
                    />
                  ))}
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
            contextRefreshKey={`${session.threadId}:${messages.length}:${session.checkpoints.length}:${sending ? 1 : 0}`}
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
            compacting={compacting}
            aboveComposer={null}
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
        focusShortcuts={shortcutsRequest}
        onClose={closeSettings}
        onChangeProvider={() => {
          closeSettings();
          setProviderSwitchOpen(true);
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

      <ProviderSwitchSheet
        open={providerSwitchOpen}
        providers={providers}
        currentProviderId={session.provider}
        busy={providerSwitchBusy}
        onClose={() => {
          if (!providerSwitchBusy) setProviderSwitchOpen(false);
        }}
        onSelect={(providerId) => {
          void (async () => {
            setProviderSwitchBusy(true);
            try {
              await onSwitchProvider(providerId);
              setProviderSwitchOpen(false);
            } catch {
              /* parent toasts */
            } finally {
              setProviderSwitchBusy(false);
            }
          })();
        }}
        onConnect={(providerId) => {
          setProviderSwitchOpen(false);
          onReconnectProvider?.(providerId);
        }}
        onRefresh={onRefreshProviders}
      />

      <WorkbenchPanel
        open={workbenchOpen}
        autoOpened={workbenchAutoOpened}
        session={session}
        messages={messages}
        sending={sending}
        compacting={compacting}
        review={workspaceReview}
        onClose={closeWorkbench}
        onFork={onForkThread}
        onVerify={onVerifyWorkspace}
        onRewind={onRewindThread}
        onJump={jumpToMessage}
      />

      <CommandPalette
        open={paletteOpen}
        actions={paletteActions}
        onClose={() => setPaletteOpen(false)}
        onCommand={(name) => {
          setPaletteOpen(false);
          onDraftChange(`/${name} `);
          requestAnimationFrame(() => focusComposer());
        }}
      />

      <DiffViewer target={diffTarget} onClose={() => setDiffTarget(null)} />
    </section>
  );
}
