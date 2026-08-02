import type { ChatEvent, ChatMessage, ToolPart } from "./types.ts";

/** Pure chat UI projection state reduced from desktop `chat-event` payloads. */
export type ChatUiState = {
  messages: ChatMessage[];
  activeAssistantId: string | null;
  sending: boolean;
  /** Accept events only for this session/thread when set. */
  sessionId: string | null;
  threadId: string | null;
  /** Current in-flight turn; mismatched turn_id events are ignored. */
  currentTurnId: string | null;
};

export type ChatReduceEffects = {
  /** App shows a toast when set (reducer stays side-effect free). */
  errorToast?: string;
  warningToast?: string;
};

export type ChatReduceResult = {
  state: ChatUiState;
  effects: ChatReduceEffects;
};

export type ChatReduceOptions = {
  /** Injected for deterministic tests; production uses crypto UUIDs. */
  newId?: (prefix: string) => string;
};

function defaultNewId(prefix: string): string {
  return `${prefix}-${crypto.randomUUID()}`;
}

function eventSessionId(event: ChatEvent): string | undefined {
  return event.session_id;
}

function eventThreadId(event: ChatEvent): string | undefined {
  return event.thread_id;
}

function eventTurnId(event: ChatEvent): string | undefined {
  const turn = event.turn_id;
  return turn == null ? undefined : turn;
}

/** Drop events that belong to a different session, thread, or turn. */
export function isStaleChatEvent(state: ChatUiState, event: ChatEvent): boolean {
  const sid = eventSessionId(event);
  const tid = eventThreadId(event);
  if (state.sessionId && sid && sid !== state.sessionId) return true;
  if (state.threadId && tid && tid !== state.threadId) return true;
  const turn = eventTurnId(event);
  if (
    state.currentTurnId &&
    turn &&
    turn !== state.currentTurnId &&
    event.kind !== "warning"
  ) {
    return true;
  }
  return false;
}

function ensureAssistant(
  state: ChatUiState,
  messageId: string | undefined,
  newId: (prefix: string) => string
): { state: ChatUiState; id: string } {
  if (messageId) {
    let messages = state.messages;
    if (!messages.some((m) => m.id === messageId)) {
      messages = [
        ...messages,
        {
          id: messageId,
          role: "assistant",
          text: "",
          thinking: "",
          tools: [],
          streaming: true,
        },
      ];
    }
    return {
      state: { ...state, messages, activeAssistantId: messageId },
      id: messageId,
    };
  }
  if (state.activeAssistantId) {
    return { state, id: state.activeAssistantId };
  }
  const id = newId("assistant");
  return {
    state: {
      ...state,
      activeAssistantId: id,
      messages: [
        ...state.messages,
        {
          id,
          role: "assistant",
          text: "",
          thinking: "",
          tools: [],
          streaming: true,
        },
      ],
    },
    id,
  };
}

function patchAssistant(
  state: ChatUiState,
  id: string,
  patch: (msg: Extract<ChatMessage, { role: "assistant" }>) => ChatMessage
): ChatUiState {
  return {
    ...state,
    messages: state.messages.map((m) =>
      m.id === id && m.role === "assistant" ? patch(m) : m
    ),
  };
}

/**
 * Characterize / apply a single chat-event to UI state.
 * Mirrors the former App.tsx `handleChatEvent` switch (no side effects).
 */
export function reduceChatEvent(
  state: ChatUiState,
  event: ChatEvent,
  options?: ChatReduceOptions
): ChatReduceResult {
  const newId = options?.newId ?? defaultNewId;
  const effects: ChatReduceEffects = {};

  if (isStaleChatEvent(state, event)) {
    return { state, effects };
  }

  switch (event.kind) {
    case "user": {
      if (state.messages.some((m) => m.id === event.message_id)) {
        return {
          state: {
            ...state,
            activeAssistantId: null,
            currentTurnId: event.turn_id,
            sending: true,
          },
          effects,
        };
      }
      return {
        state: {
          ...state,
          activeAssistantId: null,
          currentTurnId: event.turn_id,
          sending: true,
          messages: [
            ...state.messages,
            { id: event.message_id, role: "user", text: event.text },
          ],
        },
        effects,
      };
    }
    case "assistant_start": {
      const ensured = ensureAssistant(state, event.message_id, newId);
      return {
        state: {
          ...ensured.state,
          currentTurnId: event.turn_id,
          sending: true,
          activeAssistantId: ensured.id,
        },
        effects,
      };
    }
    case "text_delta": {
      const ensured = ensureAssistant(state, event.message_id, newId);
      return {
        state: patchAssistant(ensured.state, ensured.id, (m) => ({
          ...m,
          text: m.text + event.text,
          streaming: true,
        })),
        effects,
      };
    }
    case "thinking_delta": {
      const ensured = ensureAssistant(state, event.message_id, newId);
      return {
        state: patchAssistant(ensured.state, ensured.id, (m) => ({
          ...m,
          thinking: m.thinking + event.text,
          streaming: true,
        })),
        effects,
      };
    }
    case "tool_call_start": {
      const ensured = ensureAssistant(state, event.message_id, newId);
      const toolId = event.id;
      const tool: ToolPart = { id: toolId, name: event.name, status: "running" };
      return {
        state: patchAssistant(ensured.state, ensured.id, (m) => {
          if (m.tools.some((t) => t.id === toolId)) return m;
          return { ...m, tools: [...m.tools, tool], streaming: true };
        }),
        effects,
      };
    }
    case "tool_call_result": {
      const ensured = ensureAssistant(state, event.message_id, newId);
      return {
        state: patchAssistant(ensured.state, ensured.id, (m) => ({
          ...m,
          streaming: true,
          tools: m.tools.map((t) => {
            const match = event.id
              ? t.id === event.id
              : t.name === event.name &&
                (t.status === "running" || t.status === "awaiting_approval");
            if (!match) return t;
            return {
              ...t,
              status: event.isError ? "error" : "done",
              summary: event.summary,
              approvalId: undefined,
              metadata: event.metadata ?? t.metadata,
            };
          }),
        })),
        effects,
      };
    }
    case "approval_needed": {
      const ensured = ensureAssistant(state, event.message_id, newId);
      const toolId = event.tool_call_id;
      return {
        state: patchAssistant(ensured.state, ensured.id, (m) => {
          const exists = m.tools.some((t) => t.id === toolId);
          const nextTool: ToolPart = {
            id: toolId,
            name: event.tool_name,
            status: "awaiting_approval",
            summary: event.summary,
            approvalId: event.approval_id,
            path: event.path,
            diff: event.diff,
          };
          return {
            ...m,
            streaming: true,
            tools: exists
              ? m.tools.map((t) => (t.id === toolId ? { ...t, ...nextTool } : t))
              : [...m.tools, nextTool],
          };
        }),
        effects,
      };
    }
    case "done": {
      let next = state;
      if (event.message_id) {
        next = patchAssistant(next, event.message_id, (m) => ({
          ...m,
          streaming: false,
        }));
      } else if (next.activeAssistantId) {
        const id = next.activeAssistantId;
        next = patchAssistant(next, id, (m) => ({ ...m, streaming: false }));
      }
      return {
        state: {
          ...next,
          activeAssistantId: null,
          sending: false,
          currentTurnId: null,
        },
        effects,
      };
    }
    case "cancelled": {
      const ensured = ensureAssistant(state, event.message_id, newId);
      return {
        state: {
          ...patchAssistant(ensured.state, ensured.id, (m) => ({
            ...m,
            streaming: false,
            error: m.error ?? "turn cancelled",
          })),
          activeAssistantId: null,
          sending: false,
          currentTurnId: null,
        },
        effects,
      };
    }
    case "error": {
      const ensured = ensureAssistant(state, event.message_id, newId);
      effects.errorToast = event.message;
      return {
        state: {
          ...patchAssistant(ensured.state, ensured.id, (m) => ({
            ...m,
            streaming: false,
            error: event.message,
          })),
          activeAssistantId: null,
          sending: false,
          currentTurnId: null,
        },
        effects,
      };
    }
    case "warning": {
      effects.warningToast = event.message;
      return { state, effects };
    }
  }
}

export function initialChatUiState(
  messages: ChatMessage[] = [],
  identity?: { sessionId?: string | null; threadId?: string | null }
): ChatUiState {
  return {
    messages,
    activeAssistantId: null,
    sending: false,
    sessionId: identity?.sessionId ?? null,
    threadId: identity?.threadId ?? null,
    currentTurnId: null,
  };
}

/** Optimistic Allow: card becomes running until tool_call_result. */
export function markApprovalRunning(
  messages: ChatMessage[],
  approvalId: string
): ChatMessage[] {
  return messages.map((m) => {
    if (m.role !== "assistant") return m;
    return {
      ...m,
      tools: m.tools.map((t) =>
        t.approvalId === approvalId
          ? { ...t, status: "running" as const, approvalId: undefined }
          : t
      ),
    };
  });
}

/** Restore approval card after resolve_approval failed. */
export function restoreApprovalCard(
  messages: ChatMessage[],
  snapshot: ToolPart
): ChatMessage[] {
  return messages.map((m) => {
    if (m.role !== "assistant") return m;
    const idx = m.tools.findIndex((t) => t.id === snapshot.id);
    if (idx < 0) return m;
    const tools = [...m.tools];
    tools[idx] = {
      ...snapshot,
      status: "awaiting_approval",
      approvalId: snapshot.approvalId,
    };
    return { ...m, tools };
  });
}

/** Find the tool card for an approval id (for failure restore). */
export function findApprovalTool(
  messages: ChatMessage[],
  approvalId: string
): ToolPart | null {
  for (const m of messages) {
    if (m.role !== "assistant") continue;
    const tool = m.tools.find((t) => t.approvalId === approvalId);
    if (tool) return { ...tool };
  }
  return null;
}
