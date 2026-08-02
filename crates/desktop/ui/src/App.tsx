import { useCallback, useEffect, useRef, useState } from "react";

import { AuthSuccess } from "@/components/AuthSuccess";
import { ChatScreen } from "@/components/ChatScreen";
import { ProviderPicker } from "@/components/ProviderPicker";
import { WaitingScreen } from "@/components/WaitingScreen";
import { toast, Toaster } from "@/components/ui/toast";
import { BrandMark } from "@/components/BrandMark";
import { getBackend } from "@/lib/backend";
import {
  findApprovalTool,
  markApprovalRunning,
  reduceChatEvent,
  restoreApprovalCard,
} from "@/lib/chatReducer";
import { loadDraft, saveDraft } from "@/lib/drafts";
import {
  DEFAULT_CODEX_MODEL,
  DEFAULT_EFFORT,
  type EffortId,
} from "@/lib/models";
import {
  effortFromSession,
  mergeSessionOptions,
  rollbackSessionOptions,
} from "@/lib/sessionOptions";
import type { ChatEvent, ChatMessage, ProviderRow, SessionInfo, ToolPart } from "@/lib/types";
import { cn } from "@/lib/utils";

type Screen = "boot" | "picker" | "waiting" | "auth-success" | "chat";

function isReady(row: ProviderRow) {
  return row.statusKind === "ready";
}

function pickReadyProvider(rows: ProviderRow[], prefer: string | null) {
  const ready = rows.filter(isReady);
  if (prefer) {
    const preferred = ready.find((p) => p.id === prefer);
    if (preferred) return preferred;
  }
  return ready[0] ?? null;
}

const POLL_MS = 1500;
const POLL_MAX_TICKS = 120;

function newId(prefix: string) {
  return `${prefix}-${crypto.randomUUID()}`;
}

/** Collapse adjacent text/thinking deltas for the same message before reduce. */
function mergeAdjacentDeltas(events: ChatEvent[]): ChatEvent[] {
  const out: ChatEvent[] = [];
  for (const event of events) {
    const last = out[out.length - 1];
    if (
      last &&
      (event.kind === "text_delta" || event.kind === "thinking_delta") &&
      last.kind === event.kind &&
      "message_id" in last &&
      "message_id" in event &&
      last.message_id === event.message_id &&
      last.turn_id === event.turn_id &&
      "text" in last &&
      "text" in event
    ) {
      out[out.length - 1] = { ...last, text: last.text + event.text };
    } else {
      out.push(event);
    }
  }
  return out;
}

function normalizeMessages(raw: ChatMessage[] | undefined): ChatMessage[] {
  if (!raw?.length) return [];
  // Rust terminalizes interrupted tools on load; keep a belt-and-suspenders pass.
  return raw.map((msg) => {
    if (msg.role === "user") {
      return { id: msg.id, role: "user", text: msg.text ?? "" };
    }
    return {
      id: msg.id,
      role: "assistant",
      text: msg.text ?? "",
      thinking: msg.thinking ?? "",
      tools: (msg.tools ?? []).map((t): ToolPart => {
        const status =
          t.status === "awaiting_approval" || t.status === "running"
            ? "error"
            : t.status === "done" || t.status === "error"
              ? t.status
              : "done";
        return {
          id: t.id,
          name: t.name,
          status,
          summary:
            t.status === "awaiting_approval"
              ? t.summary
                ? `${t.summary} (approval interrupted)`
                : "approval interrupted"
              : t.status === "running"
                ? t.summary
                  ? `${t.summary} (interrupted)`
                  : "tool interrupted"
                : t.summary,
          path: t.path,
          diff: t.diff,
          metadata: t.metadata,
        };
      }),
      error: msg.error,
      streaming: false,
    };
  });
}

function formatInvokeError(err: unknown): string {
  const raw = String(err);
  try {
    const start = raw.indexOf("{");
    const end = raw.lastIndexOf("}");
    if (start >= 0 && end > start) {
      const parsed = JSON.parse(raw.slice(start, end + 1)) as {
        message?: string;
        code?: string;
      };
      if (parsed.message) {
        return parsed.code === "busy"
          ? `${parsed.message} — cancel the current turn first`
          : parsed.message;
      }
    }
  } catch {
    /* use raw */
  }
  return raw;
}

const backend = getBackend();

export default function App() {
  const [screen, setScreen] = useState<Screen>("boot");
  const [providers, setProviders] = useState<ProviderRow[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [pickerError, setPickerError] = useState<string | null>(null);
  const [continuing, setContinuing] = useState(false);

  const [waitingTitle, setWaitingTitle] = useState("Sign in");
  const [waitingBody, setWaitingBody] = useState(
    "Finish in your browser. This window will update when you’re done."
  );
  const [waitingHint, setWaitingHint] = useState("Waiting for browser sign-in…");
  const [waitingError, setWaitingError] = useState<string | null>(null);

  const [session, setSession] = useState<SessionInfo | null>(null);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [draft, setDraft] = useState("");
  const [sending, setSending] = useState(false);
  const [model, setModel] = useState(DEFAULT_CODEX_MODEL);
  const [effort, setEffort] = useState<EffortId>(DEFAULT_EFFORT);
  const [optionsUpdating, setOptionsUpdating] = useState(false);

  const selectedIdRef = useRef(selectedId);
  selectedIdRef.current = selectedId;
  const pollRef = useRef<number | null>(null);
  const activeAssistantId = useRef<string | null>(null);
  const messagesRef = useRef<ChatMessage[]>([]);
  messagesRef.current = messages;
  const sendingRef = useRef(sending);
  sendingRef.current = sending;
  const threadIdRef = useRef<string | null>(null);
  const sessionIdRef = useRef<string | null>(null);
  const currentTurnIdRef = useRef<string | null>(null);
  const draftRef = useRef(draft);
  draftRef.current = draft;
  const optionsUpdatingRef = useRef(false);
  const modelRef = useRef(model);
  modelRef.current = model;
  const effortRef = useRef(effort);
  effortRef.current = effort;

  const loadProviders = useCallback(async (prefer?: string | null) => {
    const rows = await backend.listProviders();
    setProviders(rows);
    setSelectedId((current) => {
      const preferId = prefer ?? current;
      if (preferId && rows.some((p) => p.id === preferId)) return preferId;
      const ready = rows.find((p) => p.statusKind === "ready");
      return ready?.id ?? rows[0]?.id ?? null;
    });
    return rows;
  }, []);

  const stopPolling = useCallback(() => {
    if (pollRef.current != null) {
      window.clearInterval(pollRef.current);
      pollRef.current = null;
    }
  }, []);

  const startWaitingPoll = useCallback(() => {
    stopPolling();
    let ticks = 0;
    setWaitingHint("Waiting for browser sign-in…");
    setWaitingError(null);

    pollRef.current = window.setInterval(async () => {
      ticks += 1;
      try {
        const rows = await loadProviders(selectedIdRef.current);
        const row = rows.find((p) => p.id === selectedIdRef.current);
        if (row?.statusKind === "ready") {
          stopPolling();
          setScreen("auth-success");
          return;
        }
      } catch {
        /* keep waiting */
      }

      if (ticks >= POLL_MAX_TICKS) {
        stopPolling();
        setWaitingHint("Still waiting");
        setWaitingError("Still waiting — complete sign-in in the browser, or Cancel.");
      }
    }, POLL_MS);
  }, [loadProviders, stopPolling]);

  const deltaQueueRef = useRef<ChatEvent[]>([]);
  const deltaRafRef = useRef<number | null>(null);

  const applyChatEventNow = useCallback((event: ChatEvent) => {
    const prevSending = sendingRef.current;
    const { state, effects } = reduceChatEvent(
      {
        messages: messagesRef.current,
        activeAssistantId: activeAssistantId.current,
        sending: prevSending,
        sessionId: sessionIdRef.current,
        threadId: threadIdRef.current,
        currentTurnId: currentTurnIdRef.current,
      },
      event,
      { newId }
    );
    messagesRef.current = state.messages;
    activeAssistantId.current = state.activeAssistantId;
    currentTurnIdRef.current = state.currentTurnId;
    setMessages(state.messages);
    if (state.sending !== prevSending) {
      sendingRef.current = state.sending;
      setSending(state.sending);
    }
    if (effects.errorToast) {
      toast.add({
        type: "error",
        title: "Request failed",
        description: effects.errorToast,
      });
    }
    if (effects.warningToast) {
      toast.add({
        type: "error",
        title: "History not saved",
        description: effects.warningToast,
      });
    }
  }, []);

  const flushDeltaQueue = useCallback(() => {
    deltaRafRef.current = null;
    const queued = deltaQueueRef.current;
    deltaQueueRef.current = [];
    // Merge adjacent text/thinking deltas before React reduce to cut renders.
    for (const event of mergeAdjacentDeltas(queued)) {
      applyChatEventNow(event);
    }
  }, [applyChatEventNow]);

  const handleChatEvent = useCallback(
    (event: ChatEvent) => {
      if (event.kind === "text_delta" || event.kind === "thinking_delta") {
        deltaQueueRef.current.push(event);
        if (deltaRafRef.current == null) {
          deltaRafRef.current = window.requestAnimationFrame(flushDeltaQueue);
        }
        return;
      }
      // Non-delta events must see coalesced text first.
      if (deltaQueueRef.current.length > 0) {
        if (deltaRafRef.current != null) {
          window.cancelAnimationFrame(deltaRafRef.current);
          deltaRafRef.current = null;
        }
        flushDeltaQueue();
      }
      applyChatEventNow(event);
    },
    [applyChatEventNow, flushDeltaQueue]
  );

  const applySession = useCallback((info: SessionInfo, opts?: { clearDraft?: boolean }) => {
    const prevThread = threadIdRef.current;
    if (prevThread && prevThread !== info.threadId) {
      saveDraft(prevThread, draftRef.current);
    }

    setSession(info);
    setSelectedId(info.provider);
    setModel(info.model);
    setEffort(effortFromSession(info.effort, DEFAULT_EFFORT));
    const messages = normalizeMessages(info.messages);
    setMessages(messages);
    messagesRef.current = messages;
    activeAssistantId.current = null;
    currentTurnIdRef.current = null;
    setSending(false);
    sendingRef.current = false;
    threadIdRef.current = info.threadId;
    sessionIdRef.current = info.sessionId;

    if (opts?.clearDraft) {
      saveDraft(info.threadId, "");
      setDraft("");
    } else {
      setDraft(loadDraft(info.threadId));
    }

    if (info.warning) {
      toast.add({
        type: "error",
        title: "Thread recovery",
        description: info.warning,
      });
    }

    setPickerError(null);
    setScreen("chat");
  }, []);

  const enterChat = useCallback(
    async (providerId: string) => {
      const info = await backend.startSession(providerId);
      stopPolling();
      applySession(info);
      return info;
    },
    [applySession, stopPolling]
  );

  const bootStarted = useRef(false);
  useEffect(() => {
    if (bootStarted.current) return;
    bootStarted.current = true;

    if (backend.mode === "fixture") {
      void (async () => {
        const info = await backend.startSession("fixture");
        applySession(info);
        setSending(true);
        sendingRef.current = true;
        try {
          await backend.boot?.(handleChatEvent);
        } finally {
          setSending(false);
          sendingRef.current = false;
        }
      })();
      return;
    }

    (async () => {
      try {
        const [rows, prefer] = await Promise.all([
          backend.listProviders(),
          backend.lastProvider().catch(() => null),
        ]);
        setProviders(rows);

        const ready = pickReadyProvider(rows, prefer);
        if (ready) {
          setSelectedId(ready.id);
          await enterChat(ready.id);
          return;
        }

        const fallback =
          (prefer && rows.find((p) => p.id === prefer)) ||
          rows.find((p) => p.statusKind === "unknown") ||
          rows[0] ||
          null;
        setSelectedId(fallback?.id ?? null);
        setScreen("picker");
      } catch (err) {
        setPickerError(String(err));
        setScreen("picker");
      }
    })();
  }, [applySession, enterChat, handleChatEvent]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    backend.onChatEvent(handleChatEvent).then((fn) => {
      if (disposed) {
        // Strict Mode: dispose late-resolving subscriptions immediately.
        fn();
        return;
      }
      unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [handleChatEvent]);

  // Persist sticky draft for the active thread.
  useEffect(() => {
    const threadId = session?.threadId;
    if (!threadId || screen !== "chat") return;
    saveDraft(threadId, draft);
  }, [draft, screen, session?.threadId]);

  useEffect(() => {
    const onFocus = () => {
      if (screen === "waiting") {
        loadProviders(selectedIdRef.current)
          .then((rows) => {
            const row = rows.find((p) => p.id === selectedIdRef.current);
            if (row?.statusKind === "ready") {
              stopPolling();
              setScreen("auth-success");
            }
          })
          .catch(() => {});
        return;
      }
      if (screen === "picker") {
        loadProviders(selectedIdRef.current).catch(() => {});
      }
    };
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [loadProviders, screen, stopPolling]);

  useEffect(() => () => stopPolling(), [stopPolling]);

  async function goContinue() {
    const row = providers.find((p) => p.id === selectedId);
    if (!row) return;
    setPickerError(null);
    setContinuing(true);
    try {
      await enterChat(row.id);
    } catch (err) {
      setScreen("picker");
      setPickerError(String(err));
    } finally {
      setContinuing(false);
    }
  }

  async function goConnect() {
    const row = providers.find((p) => p.id === selectedId);
    if (!row) return;
    setPickerError(null);
    try {
      const started = await backend.startLogin(row.id);
      setWaitingTitle(started.browserTitle);
      setWaitingBody(started.browserBody);
      setScreen("waiting");
      startWaitingPoll();
    } catch (err) {
      setPickerError(String(err));
    }
  }

  async function cancelWait() {
    stopPolling();
    setWaitingError(null);
    if (session) {
      setScreen("chat");
      return;
    }
    setScreen("picker");
    await loadProviders(selectedId);
  }

  async function changeProvider() {
    if (session?.threadId) {
      saveDraft(session.threadId, draftRef.current);
    }
    try {
      await backend.endSession();
    } catch {
      /* ignore */
    }
    setMessages([]);
    activeAssistantId.current = null;
    currentTurnIdRef.current = null;
    setSession(null);
    threadIdRef.current = null;
    sessionIdRef.current = null;
    setScreen("picker");
    await loadProviders(selectedId);
  }

  async function reconnectProvider() {
    const providerId = session?.provider ?? selectedId;
    if (!providerId || backend.mode === "fixture") return;
    setPickerError(null);
    try {
      const started = await backend.startLogin(providerId);
      setSelectedId(providerId);
      setWaitingTitle(started.browserTitle);
      setWaitingBody(started.browserBody);
      setScreen("waiting");
      startWaitingPoll();
    } catch (err) {
      toast.add({
        type: "error",
        title: "Could not start sign-in",
        description: String(err),
      });
    }
  }

  async function onNewChat() {
    try {
      if (session?.threadId) {
        saveDraft(session.threadId, draftRef.current);
      }
      const info = await backend.newThread();
      applySession(info, { clearDraft: true });
    } catch (err) {
      toast.add({
        type: "error",
        title: "Could not start new chat",
        description: String(err),
      });
    }
  }

  async function onLoadThread(id: string) {
    if (!id || id === session?.threadId) return;
    try {
      if (session?.threadId) {
        saveDraft(session.threadId, draftRef.current);
      }
      const info = await backend.loadThread(id);
      applySession(info);
    } catch (err) {
      toast.add({
        type: "error",
        title: "Could not open thread",
        description: String(err),
      });
    }
  }

  async function onSend() {
    const text = draft.trim();
    if (!text || sending) return;
    setDraft("");
    if (session?.threadId) {
      saveDraft(session.threadId, "");
    }
    // Stay busy until an authoritative done/cancelled/error chat-event arrives.
    setSending(true);
    sendingRef.current = true;
    activeAssistantId.current = null;
    try {
      await backend.sendMessage(text);
    } catch (err) {
      setSending(false);
      sendingRef.current = false;
      const message = formatInvokeError(err);
      if (!message.includes("already in progress") && !message.includes('"busy"')) {
        toast.add({
          type: "error",
          title: "Could not send",
          description: message,
        });
      } else {
        toast.add({
          type: "error",
          title: "Busy",
          description: message,
        });
      }
    }
  }

  async function onStop() {
    if (!sendingRef.current) return;
    try {
      await backend.cancelTurn();
      // Keep sending=true until the Cancelled chat-event clears busy state.
    } catch (err) {
      toast.add({
        type: "error",
        title: "Could not stop",
        description: formatInvokeError(err),
      });
    }
  }

  async function onResolveApproval(approvalId: string, allow: boolean) {
    const snapshot = findApprovalTool(messagesRef.current, approvalId);
    if (allow && snapshot) {
      const next = markApprovalRunning(messagesRef.current, approvalId);
      messagesRef.current = next;
      setMessages(next);
    }
    try {
      await backend.resolveApproval(approvalId, allow);
    } catch (err) {
      if (allow && snapshot) {
        const restored = restoreApprovalCard(messagesRef.current, snapshot);
        messagesRef.current = restored;
        setMessages(restored);
      }
      toast.add({
        type: "error",
        title: allow ? "Could not allow tool" : "Could not deny tool",
        description: String(err),
      });
      throw err;
    }
  }

  async function onModelChange(next: string) {
    if (typeof next !== "string" || !next.trim()) return;
    if (optionsUpdatingRef.current) return;
    if (screen !== "chat") {
      setModel(next);
      return;
    }
    const snapshot = { model: modelRef.current, effort: effortRef.current };
    optionsUpdatingRef.current = true;
    setOptionsUpdating(true);
    setModel(next);
    setSession((prev) => (prev ? { ...prev, model: next } : prev));
    try {
      const info = await backend.updateSessionOptions({ model: next });
      setSession((prev) => mergeSessionOptions(prev, info));
      setModel(info.model);
      setEffort(effortFromSession(info.effort, effortRef.current));
    } catch (err) {
      setSession((prev) => rollbackSessionOptions(prev, snapshot));
      setModel(snapshot.model);
      setEffort(snapshot.effort);
      toast.add({
        type: "error",
        title: "Could not update model",
        description: String(err),
      });
    } finally {
      optionsUpdatingRef.current = false;
      setOptionsUpdating(false);
    }
  }

  async function onEffortChange(next: EffortId) {
    if (optionsUpdatingRef.current) return;
    if (screen !== "chat") {
      setEffort(next);
      return;
    }
    const snapshot = { model: modelRef.current, effort: effortRef.current };
    optionsUpdatingRef.current = true;
    setOptionsUpdating(true);
    setEffort(next);
    setSession((prev) => (prev ? { ...prev, effort: next } : prev));
    try {
      const info = await backend.updateSessionOptions({ effort: next });
      setSession((prev) => mergeSessionOptions(prev, info));
      setModel(info.model);
      setEffort(effortFromSession(info.effort, snapshot.effort));
    } catch (err) {
      setSession((prev) => rollbackSessionOptions(prev, snapshot));
      setModel(snapshot.model);
      setEffort(snapshot.effort);
      toast.add({
        type: "error",
        title: "Could not update effort",
        description: String(err),
      });
    } finally {
      optionsUpdatingRef.current = false;
      setOptionsUpdating(false);
    }
  }

  const authMode = screen !== "chat";

  return (
    <Toaster>
      <div
        className={cn(
          "h-full w-full",
          authMode && "flex items-center justify-center px-6 py-8",
          !authMode && "flex min-h-0 flex-col"
        )}
      >
        {screen === "boot" ? (
          <section className="flex w-full max-w-[420px] flex-col items-center text-center">
            <BrandMark />
            <p className="mt-4 text-sm text-muted-foreground">Opening your session…</p>
          </section>
        ) : null}

        {screen === "picker" ? (
          <ProviderPicker
            providers={providers}
            selectedId={selectedId}
            error={pickerError}
            onSelect={setSelectedId}
            onContinue={goContinue}
            onConnect={goConnect}
            continuing={continuing}
          />
        ) : null}

        {screen === "waiting" ? (
          <WaitingScreen
            title={waitingTitle}
            body={waitingBody}
            hint={waitingHint}
            error={waitingError}
            onCancel={cancelWait}
          />
        ) : null}

        {screen === "auth-success" ? (
          <AuthSuccess onContinue={goContinue} continuing={continuing} />
        ) : null}

        {screen === "chat" && session ? (
          <ChatScreen
            session={session}
            messages={messages}
            draft={draft}
            sending={sending}
            model={model}
            effort={effort}
            optionsDisabled={optionsUpdating}
            onDraftChange={setDraft}
            onSend={onSend}
            onStop={onStop}
            onNewChat={onNewChat}
            onChangeProvider={changeProvider}
            onReconnect={reconnectProvider}
            onLoadThread={onLoadThread}
            onModelChange={onModelChange}
            onEffortChange={onEffortChange}
            onResolveApproval={onResolveApproval}
          />
        ) : null}
      </div>
    </Toaster>
  );
}
