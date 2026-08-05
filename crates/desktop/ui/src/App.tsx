import { useCallback, useEffect, useRef, useState } from "react";

import { AuthSuccess } from "@/components/AuthSuccess";
import { ChatScreen } from "@/components/ChatScreen";
import { ChatSkeleton } from "@/components/ChatSkeleton";
import { ProfileScreen } from "@/components/ProfileScreen";
import { ProviderPicker } from "@/components/ProviderPicker";
import { WaitingScreen } from "@/components/WaitingScreen";
import { toast, Toaster } from "@/components/ui/toast";
import { getBackend } from "@/lib/backend";
import {
  findApprovalTool,
  markApprovalRunning,
  reduceChatEvent,
  restoreApprovalCard,
} from "@/lib/chatReducer";
import { loadDraft, saveDraft } from "@/lib/drafts";
import { isLongTurn } from "@/lib/notificationPolicy";
import { isWindowActuallyActive, notifyWhenAway } from "@/lib/notifications";
import { revealCount } from "@/lib/reveal";
import {
  DEFAULT_CODEX_MODEL,
  DEFAULT_EFFORT,
  type EffortId,
} from "@/lib/models";
import {
  markProviderVerified,
  markProviderVerifyFailed,
  recentVerifyFailed,
  recentVerifySucceeded,
} from "@/lib/providerVerify";
import {
  effortFromSession,
  mergeSessionOptions,
  rollbackSessionOptions,
} from "@/lib/sessionOptions";
import { markStartup, measureStartup } from "@/lib/startupPerf";
import type {
  ApprovalChoice,
  ApprovalMode,
  ChatEvent,
  ChatMessage,
  PreparedAttachment,
  ProviderRow,
  SessionInfo,
  SessionWarning,
  ToolPart,
  UserAttachmentChip,
  UserProfile,
} from "@/lib/types";
import { cn } from "@/lib/utils";

type Screen = "boot" | "picker" | "waiting" | "auth-success" | "chat" | "profile";

function isReady(row: ProviderRow) {
  // Soft memory: a recent failed probe beats filesystem "Signed in".
  return row.statusKind === "ready" && !recentVerifyFailed(row.id);
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

async function showAttention(
  title: string,
  description: string,
  type: "warning" | "success"
) {
  if (await isWindowActuallyActive()) {
    toast.add({ type, title, description });
  } else {
    await notifyWhenAway(title, description);
  }
}

/**
 * What "Build plan" says on the user's behalf.
 *
 * It lands in the transcript as their message, so it is worded as something a
 * person would say — clicking the button *is* saying this, and the transcript
 * should not contain instructions they never gave.
 */
const BUILD_PLAN_PROMPT =
  "Build the plan. Delegate the steps you marked as suiting another model; " +
  "where routing has no match, build it here instead.";

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
      return {
        id: msg.id,
        role: "user",
        text: msg.text ?? "",
        attachments: msg.attachments,
      };
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
      // Persisted, so a reopened plan still renders as a plan.
      command: msg.command,
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

function shouldOfferProviderReconnect(err: unknown): boolean {
  const message = formatInvokeError(err).toLowerCase();
  return message.includes("needs connect again") || message.includes("auth_unavailable");
}

const backend = getBackend();

export default function App() {
  const [screen, setScreen] = useState<Screen>("boot");
  /** Bumped to ask ChatScreen to open Settings at the User section. */
  const [settingsRequest, setSettingsRequest] = useState(0);
  const [providers, setProviders] = useState<ProviderRow[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  /**
   * A background verification that failed after the chat had already opened.
   *
   * Shown in the chat rather than bouncing back to the picker: the session is
   * real and the transcript is readable, so throwing the user out would lose
   * more than the warning gains.
   */
  const [sessionWarning, setSessionWarning] = useState<SessionWarning | null>(null);
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
  const [attachments, setAttachments] = useState<PreparedAttachment[]>([]);
  const [workspacePath, setWorkspacePath] = useState<string | null>(null);
  const [branch, setBranch] = useState<string | null>(null);
  const [profile, setProfile] = useState<UserProfile>({
    displayName: "",
    avatarDataUrl: "",
  });
  const [sending, setSending] = useState(false);
  const [model, setModel] = useState(DEFAULT_CODEX_MODEL);
  const [effort, setEffort] = useState<EffortId>(DEFAULT_EFFORT);
  // Mirrors DESKTOP_DEFAULT_MODE in Rust; reconciled on session start.
  const [approvalModeState, setApprovalModeState] =
    useState<ApprovalMode>("auto");
  const [optionsUpdating, setOptionsUpdating] = useState(false);
  /**
   * The mode Plan mode interrupted, restored by Build.
   *
   * `null` means planning was never entered from somewhere else this session —
   * the app opened in Plan, or it was restored from disk. Build then falls back
   * to the desktop default rather than inventing a permission level.
   */
  const modeBeforePlanRef = useRef<ApprovalMode | null>(null);

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
  const turnStartedAtRef = useRef<number | null>(null);
  const notifiedApprovalIdsRef = useRef(new Set<string>());
  const draftRef = useRef(draft);
  draftRef.current = draft;
  const attachmentsRef = useRef(attachments);
  attachmentsRef.current = attachments;
  const optionsUpdatingRef = useRef(false);
  const modelRef = useRef(model);
  modelRef.current = model;
  const effortRef = useRef(effort);
  effortRef.current = effort;
  /** Set just before send; applied when the matching user_message event arrives. */
  const pendingUserAttachmentsRef = useRef<UserAttachmentChip[] | null>(null);
  const enterChatRef = useRef<(providerId: string) => Promise<SessionInfo>>(
    async () => {
      throw new Error("session start is not ready yet");
    }
  );

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

  const finishVerifiedLogin = useCallback(async (row: ProviderRow) => {
    // File presence is not a working session — prove it, then open chat
    // without an extra Continue click.
    setWaitingHint("Checking the sign-in works…");
    try {
      await backend.verifyProvider(row.id);
    } catch (err) {
      markProviderVerifyFailed(row.id);
      setWaitingHint("Signed in, but the provider still refused");
      setWaitingError(
        `${row.label} accepted the sign-in but cannot serve a request yet. ` +
          `Try connecting again.\n\n${String(err)}`
      );
      return;
    }
    markProviderVerified(row.id);
    setWaitingHint("Opening chat…");
    try {
      await enterChatRef.current(row.id);
    } catch (err) {
      markProviderVerifyFailed(row.id);
      setPickerError(String(err));
      setScreen("picker");
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
        // Ready, or a session file appeared but looked incomplete — either way
        // prove it with a probe instead of spinning on "Waiting…".
        const fileAppeared =
          row?.statusKind === "ready" ||
          (row?.statusKind === "not_logged_in" &&
            row.detail.toLowerCase().includes("incomplete"));
        if (row && fileAppeared) {
          stopPolling();
          await finishVerifiedLogin(row);
          return;
        }

        const login = await backend.loginStatus();
        if (login.state === "exited") {
          stopPolling();
          setWaitingHint("Sign-in stopped");
          setWaitingError(
            login.detail ??
              "The browser sign-in stopped before Zest received the credentials."
          );
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
  }, [finishVerifiedLogin, loadProviders, stopPolling]);

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
    // Attach filename chips from the send that produced this user event.
    // History reload omits them until thread persistence gains attachments.
    let nextMessages = state.messages;
    if (event.kind === "user" && pendingUserAttachmentsRef.current) {
      const chips = pendingUserAttachmentsRef.current;
      pendingUserAttachmentsRef.current = null;
      nextMessages = nextMessages.map((m) =>
        m.role === "user" && m.id === event.message_id
          ? { ...m, attachments: chips }
          : m
      );
    }

    messagesRef.current = nextMessages;
    activeAssistantId.current = state.activeAssistantId;
    currentTurnIdRef.current = state.currentTurnId;
    setMessages(nextMessages);
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
        type: "warning",
        title: "History not saved",
        description: effects.warningToast,
      });
    }

    if (event.kind === "user" && state.currentTurnId === event.turn_id) {
      turnStartedAtRef.current = Date.now();
      notifiedApprovalIdsRef.current.clear();
    }

    if (event.kind === "approval_needed") {
      if (!notifiedApprovalIdsRef.current.has(event.approval_id)) {
        notifiedApprovalIdsRef.current.add(event.approval_id);
        const description = event.summary
          ? `${event.tool_name}: ${event.summary}`
          : `${event.tool_name} is waiting for your approval.`;
        void showAttention("Approval needed", description, "warning");
      }
    }

    if (event.kind === "done") {
      const startedAt = turnStartedAtRef.current;
      if (startedAt != null && isLongTurn(Date.now() - startedAt)) {
        void showAttention("Response ready", "Zest finished the turn.", "success");
      }
      turnStartedAtRef.current = null;
      notifiedApprovalIdsRef.current.clear();
    }

    if (event.kind === "error" || event.kind === "cancelled") {
      turnStartedAtRef.current = null;
      notifiedApprovalIdsRef.current.clear();
    }
  }, []);

  const flushDeltaQueue = useCallback(
    (drainAll = false) => {
      deltaRafRef.current = null;
      const queued = deltaQueueRef.current;
      deltaQueueRef.current = [];
      // Merge adjacent text/thinking deltas before React reduce to cut renders.
      const merged = mergeAdjacentDeltas(queued);

      for (let i = 0; i < merged.length; i += 1) {
        const event = merged[i];
        if (!drainAll && event.kind === "text_delta") {
          const reveal = revealCount(event.text.length);
          if (reveal < event.text.length) {
            applyChatEventNow({ ...event, text: event.text.slice(0, reveal) });
            // Order matters: the remainder and everything queued behind it
            // wait a frame together, or later text would overtake earlier text.
            deltaQueueRef.current = [
              { ...event, text: event.text.slice(reveal) },
              ...merged.slice(i + 1),
            ];
            break;
          }
        }
        applyChatEventNow(event);
      }

      if (deltaQueueRef.current.length > 0 && deltaRafRef.current == null) {
        deltaRafRef.current = window.requestAnimationFrame(() =>
          flushDeltaQueue()
        );
      }
    },
    [applyChatEventNow]
  );

  const handleChatEvent = useCallback(
    (event: ChatEvent) => {
      if (event.kind === "text_delta" || event.kind === "thinking_delta") {
        deltaQueueRef.current.push(event);
        if (deltaRafRef.current == null) {
          deltaRafRef.current = window.requestAnimationFrame(() =>
            flushDeltaQueue()
          );
        }
        return;
      }
      // Non-delta events must see coalesced text first — and all of it, so a
      // partially revealed buffer never lands after the `done` that ended it.
      if (deltaQueueRef.current.length > 0) {
        if (deltaRafRef.current != null) {
          window.cancelAnimationFrame(deltaRafRef.current);
          deltaRafRef.current = null;
        }
        flushDeltaQueue(true);
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
    setWorkspacePath(info.root);
    void backend.gitBranch().then(setBranch).catch(() => setBranch(null));
    setSelectedId(info.provider);
    setModel(info.model);
    setEffort(effortFromSession(info.effort, DEFAULT_EFFORT));
    // Rust owns the mode and it survives project switches, so read it back
    // rather than assuming the chip still matches.
    void backend
      .approvalMode()
      .then((mode) => setApprovalModeState(mode as ApprovalMode))
      .catch(() => {
        /* keep the current chip; the picker is not worth an error toast */
      });
    const messages = normalizeMessages(info.messages);
    setMessages(messages);
    messagesRef.current = messages;
    activeAssistantId.current = null;
    currentTurnIdRef.current = null;
    turnStartedAtRef.current = null;
    notifiedApprovalIdsRef.current.clear();
    setSending(false);
    sendingRef.current = false;
    threadIdRef.current = info.threadId;
    sessionIdRef.current = info.sessionId;
    setAttachments([]);

    if (opts?.clearDraft) {
      saveDraft(info.threadId, "");
      setDraft("");
    } else {
      setDraft(loadDraft(info.threadId));
    }

    if (info.warning) {
      toast.add({
        type: "warning",
        title: "Thread recovery",
        description: info.warning,
      });
    }

    setPickerError(null);
    setScreen("chat");
  }, []);

  /**
   * Prove the account can serve, without making anyone wait for it.
   *
   * `startSession` no longer probes, so this is where a cooled-down session is
   * discovered. It runs behind an already-usable chat and reports itself in a
   * banner, because a live turn against the provider is a network round trip and
   * gating the first paint on one is what made launch feel slow.
   */
  const verifyInBackground = useCallback((providerId: string) => {
    // A recent success is worth trusting; re-learning it costs a real turn.
    if (recentVerifySucceeded(providerId)) {
      setSessionWarning(null);
      return;
    }
    void backend
      .verifyProvider(providerId)
      .then(() => {
        markProviderVerified(providerId);
        setSessionWarning(null);
      })
      .catch((err: unknown) => {
        const offerReconnect = shouldOfferProviderReconnect(err);
        if (offerReconnect) markProviderVerifyFailed(providerId);
        setSessionWarning({
          providerId,
          message: formatInvokeError(err),
          offerReconnect,
        });
      });
  }, []);

  const enterChat = useCallback(
    async (providerId: string) => {
      try {
        const info = await backend.startSession(providerId);
        stopPolling();
        applySession(info);
        // After the chat is on screen, never before it.
        verifyInBackground(providerId);
        return info;
      } catch (err) {
        // Setup failures (for example, a new folder without a provider config)
        // are not authentication failures. Marking every start error as a
        // failed verification made the picker incorrectly say "Reconnect".
        if (shouldOfferProviderReconnect(err)) {
          markProviderVerifyFailed(providerId);
        }
        throw err;
      }
    },
    [applySession, stopPolling, verifyInBackground]
  );
  enterChatRef.current = enterChat;

  const bootStarted = useRef(false);
  useEffect(() => {
    if (bootStarted.current) return;
    bootStarted.current = true;
    markStartup("boot-effect");

    // Before any turn is recorded: core buckets usage by day, and only the
    // webview knows what day it is here. Failing is not worth blocking boot —
    // the cost is buckets landing on UTC days instead of local ones.
    void backend.setLocalOffset().catch(() => {});

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
        const [rows, prefer, folder, userProfile] = await Promise.all([
          backend.listProviders(),
          backend.lastProvider().catch(() => null),
          backend.getWorkspaceFolder().catch(() => null),
          backend.getUserProfile().catch(() => ({
            displayName: "",
            avatarDataUrl: "",
          })),
        ]);
        setProviders(rows);
        markStartup("backend-ready");
        measureStartup("backend-ready", "boot-effect");
        if (folder) setWorkspacePath(folder);
        setProfile(userProfile);
        void backend.gitBranch().then(setBranch).catch(() => setBranch(null));

        const ready = pickReadyProvider(rows, prefer);
        if (ready) {
          setSelectedId(ready.id);
          try {
            // startSession probes gateway providers; catch here so a dead Claude
            // session lands on the picker with Connect instead of a chat error.
            await enterChat(ready.id);
            markStartup("session-ready");
            measureStartup("session-ready", "boot-effect");
            return;
          } catch (err) {
            setPickerError(String(err));
            setScreen("picker");
            markStartup("picker-error");
            measureStartup("picker-error", "boot-effect");
            return;
          }
        }

        const fallback =
          (prefer && rows.find((p) => p.id === prefer)) ||
          rows.find((p) => p.statusKind === "unknown") ||
          rows[0] ||
          null;
        setSelectedId(fallback?.id ?? null);
        setScreen("picker");
        markStartup("picker-ready");
        measureStartup("picker-ready", "boot-effect");
      } catch (err) {
        setPickerError(String(err));
        setScreen("picker");
        markStartup("picker-error");
        measureStartup("picker-error", "boot-effect");
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
        void (async () => {
          try {
            const rows = await loadProviders(selectedIdRef.current);
            const row = rows.find((p) => p.id === selectedIdRef.current);
            if (!row) return;
            const fileAppeared =
              row.statusKind === "ready" ||
              (row.statusKind === "not_logged_in" &&
                row.detail.toLowerCase().includes("incomplete"));
            if (!fileAppeared) return;
            stopPolling();
            await finishVerifiedLogin(row);
          } catch {
            /* keep waiting */
          }
        })();
        return;
      }
      if (screen === "picker") {
        loadProviders(selectedIdRef.current).catch(() => {});
      }
    };
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [finishVerifiedLogin, loadProviders, screen, stopPolling]);

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
    await backend.cancelLogin().catch(() => {});
    setWaitingError(null);
    if (session) {
      setScreen("chat");
      return;
    }
    setScreen("picker");
    await loadProviders(selectedId);
  }

  async function switchProvider(providerId: string) {
    if (!providerId || providerId === session?.provider) return;
    if (session?.threadId) {
      saveDraft(session.threadId, draftRef.current);
    }
    setSelectedId(providerId);
    try {
      await enterChat(providerId);
    } catch (err) {
      setPickerError(String(err));
      toast.add({
        type: "error",
        title: "Could not switch provider",
        description: String(err),
      });
      throw err;
    }
  }

  /** `only` targets a specific provider — used by the Reconnect on an auth
   *  failure, which knows exactly which account the gateway rejected. */
  async function reconnectProvider(only?: string) {
    const providerId = only ?? session?.provider ?? selectedId;
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

  async function onDeleteThread(id: string, projectPath: string) {
    try {
      const deletedActive = session?.threadId === id;
      const info = await backend.deleteThread(id, projectPath);
      saveDraft(id, "");
      // Always refresh when the open thread was deleted (path strings from the
      // sidebar may not match session.root byte-for-byte on Windows).
      if (deletedActive || info.threadId !== session?.threadId) {
        applySession(info, { clearDraft: true });
      }
      setWorkspacePath(info.root);
      void backend.gitBranch().then(setBranch).catch(() => setBranch(null));
      toast.add({
        type: "success",
        title: "Chat deleted",
        description: deletedActive
          ? "No new chat saved — type to start one"
          : undefined,
      });
    } catch (err) {
      toast.add({
        type: "error",
        title: "Could not delete chat",
        description: String(err),
      });
      throw err;
    }
  }

  async function onOpenProjectChat(options: {
    root: string;
    threadId?: string;
    newThread?: boolean;
  }) {
    if (sendingRef.current) {
      toast.add({
        type: "error",
        title: "Busy",
        description: "Stop the current turn before switching project",
      });
      return;
    }
    try {
      if (session?.threadId) {
        saveDraft(session.threadId, draftRef.current);
      }
      const info = await backend.openProjectChat(options);
      applySession(info, { clearDraft: Boolean(options.newThread) });
      setWorkspacePath(info.root);
      void backend.gitBranch().then(setBranch).catch(() => setBranch(null));
    } catch (err) {
      toast.add({
        type: "error",
        title: "Could not open project chat",
        description: formatInvokeError(err),
      });
      throw err;
    }
  }

  function mergeAttachments(files: PreparedAttachment[]) {
    setAttachments((prev) => {
      const seen = new Set(prev.map((a) => a.path + a.name + (a.dataBase64?.slice(0, 32) ?? "")));
      const next = files.filter(
        (f) => !seen.has(f.path + f.name + (f.dataBase64?.slice(0, 32) ?? ""))
      );
      return [...prev, ...next];
    });
    for (const file of files) {
      if (file.status === "error") {
        toast.add({
          type: "error",
          title: file.name,
          description: file.detail,
        });
      }
    }
  }

  async function onAttachFiles() {
    try {
      const files = await backend.pickFiles();
      if (!files.length) return;
      mergeAttachments(files);
    } catch (err) {
      toast.add({
        type: "error",
        title: "Could not attach files",
        description: formatInvokeError(err),
      });
    }
  }

  async function onPasteImages(files: File[]) {
    try {
      const prepared: PreparedAttachment[] = [];
      for (const file of files) {
        const dataUrl = await new Promise<string>((resolve, reject) => {
          const reader = new FileReader();
          reader.onload = () =>
            resolve(typeof reader.result === "string" ? reader.result : "");
          reader.onerror = () => reject(reader.error ?? new Error("read failed"));
          reader.readAsDataURL(file);
        });
        const base64 = dataUrl.includes(",") ? dataUrl.split(",").pop()! : dataUrl;
        const att = await backend.preparePastedImage({
          dataBase64: base64,
          mediaType: file.type || "image/png",
          name: file.name || undefined,
        });
        prepared.push(att);
      }
      if (prepared.length) mergeAttachments(prepared);
    } catch (err) {
      toast.add({
        type: "error",
        title: "Could not paste image",
        description: formatInvokeError(err),
      });
    }
  }

  async function onOpenFolder() {
    if (sendingRef.current) {
      toast.add({
        type: "error",
        title: "Busy",
        description: "Stop the current turn before changing folder",
      });
      return;
    }
    try {
      const result = await backend.pickWorkspaceFolder();
      if (!result) return;
      setWorkspacePath(result.path);
      void backend.gitBranch().then(setBranch).catch(() => setBranch(null));
      if (result.sessionEnded || session) {
        const providerId = session?.provider ?? selectedId;
        if (providerId) await loadProviders(providerId);
        setSession(null);
        sessionIdRef.current = null;
        threadIdRef.current = null;
        setMessages([]);
        setAttachments([]);
        if (providerId) {
          try {
            await enterChat(providerId);
          } catch (err) {
            setPickerError(formatInvokeError(err));
            setScreen("picker");
          }
        } else {
          setScreen("picker");
        }
      }
    } catch (err) {
      toast.add({
        type: "error",
        title: "Could not open folder",
        description: formatInvokeError(err),
      });
    }
  }

  async function onSend() {
    const text = draft.trim();
    const pending = attachmentsRef.current;
    const hasOk = pending.some(
      (a) =>
        a.status === "done" &&
        (Boolean(a.content?.trim()) || (a.kind === "image" && Boolean(a.dataBase64)))
    );
    if ((!text && !hasOk) || sending) return;
    const chips: UserAttachmentChip[] = pending
      .filter((a) => a.status === "done")
      .map((a) => ({ name: a.name, kind: a.kind }));
    pendingUserAttachmentsRef.current = chips.length > 0 ? chips : null;
    setDraft("");
    setAttachments([]);
    if (session?.threadId) {
      saveDraft(session.threadId, "");
    }
    await submitTurn(text, pending, { restoreDraftOnFailure: true });
  }

  /**
   * Send one turn. Split out of `onSend` so a button can start a turn without
   * going through the composer — the composer owns the draft and attachments,
   * and a turn does not have to come from either.
   */
  async function submitTurn(
    text: string,
    pending: PreparedAttachment[],
    { restoreDraftOnFailure }: { restoreDraftOnFailure: boolean }
  ) {
    // Stay busy until an authoritative done/cancelled/error chat-event arrives.
    setSending(true);
    sendingRef.current = true;
    activeAssistantId.current = null;
    try {
      await backend.sendMessage(
        text,
        pending.map((a) => ({
          name: a.name,
          detail: a.detail,
          content: a.content,
          status: a.status,
          kind: a.kind,
          mediaType: a.mediaType,
          dataBase64: a.dataBase64,
        }))
      );
    } catch (err) {
      pendingUserAttachmentsRef.current = null;
      setSending(false);
      sendingRef.current = false;
      // Only text the user typed goes back in the composer. Putting a
      // button's prompt there would leave them holding words they never wrote.
      if (restoreDraftOnFailure) {
        setDraft(text);
        setAttachments(pending);
        if (session?.threadId) {
          saveDraft(session.threadId, text);
        }
      }
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

  /**
   * Leave Plan mode and tell the model to build what it just planned.
   *
   * Delegation happens here rather than during planning: there is nothing to
   * hand a worker until the plan exists, and a worker sees none of this
   * conversation. The plan already names which steps suit another model, so the
   * prompt only has to say "use it where you marked it" — and if routing offers
   * no match, `delegate` reports that and the model builds it inline.
   */
  async function onBuildPlan() {
    if (sendingRef.current) return;

    if (approvalModeState === "plan") {
      // Restore rather than escalate. Auto is the fallback only because it is
      // the mode the desktop opens in, so it is the one the user has already
      // consented to by default.
      const target = modeBeforePlanRef.current ?? "auto";
      modeBeforePlanRef.current = null;
      try {
        await backend.setApprovalMode(target);
        setApprovalModeState(target);
      } catch (err) {
        // Building under Plan mode would fail every write, so stop here and
        // leave the user in a mode they can see.
        toast.add({
          type: "error",
          title: "Could not leave Plan mode",
          description: String(err),
        });
        return;
      }
    }

    await submitTurn(BUILD_PLAN_PROMPT, [], { restoreDraftOnFailure: false });
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

  async function onResolveApproval(
    approvalId: string,
    decision: ApprovalChoice
  ) {
    const allow = decision !== "deny";
    const snapshot = findApprovalTool(messagesRef.current, approvalId);
    if (allow && snapshot) {
      const next = markApprovalRunning(messagesRef.current, approvalId);
      messagesRef.current = next;
      setMessages(next);
    }
    try {
      await backend.resolveApproval(approvalId, decision);
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

  async function onApprovalModeChange(next: ApprovalMode) {
    const previous = approvalModeState;
    // Remember what planning interrupted, so Build can put it back rather than
    // picking a permission level on the user's behalf.
    if (next === "plan" && previous !== "plan") {
      modeBeforePlanRef.current = previous;
    }
    setApprovalModeState(next);
    try {
      await backend.setApprovalMode(next);
    } catch (err) {
      // Rust is authoritative — put the picker back if it refused.
      setApprovalModeState(previous);
      toast.add({
        type: "error",
        title: "Could not change mode",
        description: String(err),
      });
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
          authMode &&
            "relative flex items-center justify-center overflow-auto px-6 py-8 before:pointer-events-none before:absolute before:inset-0 before:z-0 before:bg-[radial-gradient(ellipse_at_50%_0%,color-mix(in_srgb,var(--primary)_10%,transparent),transparent_55%)] [&>*]:relative [&>*]:z-10",
          !authMode && "flex min-h-0 flex-col"
        )}
      >
        {/* Boot is short now that session start no longer waits on the network. */}
        {screen === "boot" ? <ChatSkeleton /> : null}

        {screen === "picker" ? (
          <ProviderPicker
            providers={providers}
            selectedId={selectedId}
            workspacePath={workspacePath}
            error={pickerError}
            onSelect={setSelectedId}
            onContinue={goContinue}
            onConnect={goConnect}
            onOpenFolder={onOpenFolder}
            onRefresh={() => loadProviders(selectedIdRef.current).then(() => undefined)}
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
            attachments={attachments}
            branch={branch}
            profile={profile}
            sending={sending}
            model={model}
            effort={effort}
            optionsDisabled={optionsUpdating}
            onDraftChange={setDraft}
            onSend={onSend}
            onStop={onStop}
            onNewChat={onNewChat}
            onDeleteThread={onDeleteThread}
            onOpenProjectChat={onOpenProjectChat}
            providers={providers}
            onSwitchProvider={switchProvider}
            onRefreshProviders={() =>
              loadProviders(session?.provider ?? selectedIdRef.current).then(() => undefined)
            }
            onReconnect={reconnectProvider}
            onLoadThread={onLoadThread}
            onAttachFiles={onAttachFiles}
            onOpenFolder={onOpenFolder}
            onRemoveAttachment={(id) =>
              setAttachments((prev) => prev.filter((a) => a.id !== id))
            }
            onPasteImages={onPasteImages}
            onProfileChange={setProfile}
            onModelChange={onModelChange}
            onEffortChange={onEffortChange}
            onResolveApproval={onResolveApproval}
            onReconnectProvider={(providerId) => {
              // Same path as the picker Connect: spawns the vendor/gateway
              // login and shows the waiting screen until it resolves.
              void reconnectProvider(providerId);
            }}
            onReloadSession={async () => {
              // Rebuilds the runtime so a routing change takes effect. The
              // sticky thread is reloaded, so the open chat survives.
              const id = session?.provider ?? selectedIdRef.current;
              if (!id) return;
              try {
                await enterChat(id);
              } catch (err) {
                setPickerError(String(err));
                setScreen("picker");
              }
            }}
            approvalMode={approvalModeState}
            onApprovalModeChange={onApprovalModeChange}
            onBuildPlan={() => void onBuildPlan()}
            onOpenProfile={() => setScreen("profile")}
            settingsRequest={settingsRequest}
            sessionWarning={sessionWarning}
            onDismissWarning={() => setSessionWarning(null)}
          />
        ) : null}

        {screen === "profile" ? (
          <ProfileScreen
            profile={profile}
            providerLabel={
              providers.find((p) => p.id === session?.provider)?.label ?? session?.provider
            }
            onBack={() => setScreen("chat")}
            onEditProfile={() => {
              // Editing name and avatar stays in Settings; the profile screen
              // reports, it does not duplicate the form.
              setScreen("chat");
              setSettingsRequest((n) => n + 1);
            }}
          />
        ) : null}
      </div>
    </Toaster>
  );
}
