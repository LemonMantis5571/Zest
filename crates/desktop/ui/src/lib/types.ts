import type { ChatEvent as GeneratedChatEvent } from "./generated/ChatEvent.ts";
import type { SessionInfo as GeneratedSessionInfo } from "./generated/SessionInfo.ts";

export type StatusKind = "ready" | "unknown" | "not_logged_in" | "unconfigured";

export type ProviderRow = {
  id: string;
  label: string;
  method: string;
  statusKind: StatusKind;
  statusLabel: string;
  detail: string;
  selectable: boolean;
  canConnect: boolean;
};

export type LoginStarted = {
  browserTitle: string;
  browserBody: string;
};

export type ToolPart = {
  id: string;
  name: string;
  status: "running" | "awaiting_approval" | "done" | "error";
  summary?: string;
  approvalId?: string;
  path?: string;
  diff?: string;
};

export type ChatMessage =
  | { id: string; role: "user"; text: string }
  | {
      id: string;
      role: "assistant";
      text: string;
      thinking: string;
      tools: ToolPart[];
      error?: string;
      streaming: boolean;
    };

/** Wire shape from Rust `StoredMessage` (role-tagged). */
export type StoredMessage = ChatMessage;

/**
 * Wire chat events from Rust (`ChatEvent` in zest-desktop).
 * Regenerate: see `./generated/README.md`.
 */
export type ChatEvent = GeneratedChatEvent;

/** Session snapshot from Rust; messages are the UI ChatMessage projection. */
export type SessionInfo = Omit<GeneratedSessionInfo, "messages"> & {
  messages: ChatMessage[];
};

export type ThreadSummary = {
  id: string;
  createdAt: number;
  updatedAt: number;
  title?: string;
  messageCount: number;
};

/** Identity fields present on most chat-event variants. */
export type EventIdentity = {
  session_id: string;
  thread_id: string;
  turn_id: string;
};
