import type { ChatEvent as GeneratedChatEvent } from "./generated/ChatEvent.ts";
import type { ModelCapability } from "./generated/ModelCapability.ts";
import type { ProviderView as GeneratedProviderView } from "./generated/ProviderView.ts";
import type { SessionInfo as GeneratedSessionInfo } from "./generated/SessionInfo.ts";
import type { ToolMetaView } from "./generated/ToolMetaView.ts";

export type StatusKind = "ready" | "unknown" | "not_logged_in" | "unconfigured";

/** Rust-authoritative provider row (auth + catalogue). */
export type ProviderRow = Omit<GeneratedProviderView, "statusKind"> & {
  statusKind: StatusKind;
};

export type LoginStarted = {
  browserTitle: string;
  browserBody: string;
};

export type { ModelCapability };

export type ToolMetadata = ToolMetaView;

export type ToolPart = {
  id: string;
  name: string;
  status: "running" | "awaiting_approval" | "done" | "error";
  summary?: string;
  approvalId?: string;
  path?: string;
  diff?: string;
  metadata?: ToolMetadata;
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
  providerId?: string;
  messageCount: number;
};

/** Identity fields present on most chat-event variants. */
export type EventIdentity = {
  session_id: string;
  thread_id: string;
  turn_id: string;
};

export type MeasuredUsage = {
  label: string;
  requests: number;
  inputTokens: number;
  outputTokens: number;
  cacheWriteTokens: number;
  cacheReadTokens: number;
  totalTokens: number;
};

export type HeadroomView =
  | {
      kind: "provider_reported";
      label: string;
      ageSecs?: number | null;
      requestsRemaining?: number | null;
      inputTokensRemaining?: number | null;
      outputTokensRemaining?: number | null;
      retryAfterSecs?: number | null;
    }
  | { kind: "not_reported"; label: string };

export type ProviderUsageView = {
  providerId: string;
  measured: MeasuredUsage;
  headroom: HeadroomView;
};

export type UsageSnapshot = {
  providers: ProviderUsageView[];
  path?: string | null;
};
