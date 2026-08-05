import type { ChatEvent as GeneratedChatEvent } from "./generated/ChatEvent.ts";
import type { ModelCapability } from "./generated/ModelCapability.ts";
import type { CommandView } from "./generated/CommandView.ts";
import type { ExternalAgentCheckView } from "./generated/ExternalAgentCheckView.ts";
import type { ExternalAgentView } from "./generated/ExternalAgentView.ts";
import type { ProviderModelsView } from "./generated/ProviderModelsView.ts";
import type { ProviderView as GeneratedProviderView } from "./generated/ProviderView.ts";
import type { RoutingRuleView } from "./generated/RoutingRuleView.ts";
import type { RoutingView } from "./generated/RoutingView.ts";
import type { SessionInfo as GeneratedSessionInfo } from "./generated/SessionInfo.ts";
import type { ThreadCheckpointView } from "./generated/ThreadCheckpoint.ts";
import type { ToolMetaView } from "./generated/ToolMetaView.ts";
import type { WorkspaceReview as GeneratedWorkspaceReview } from "./generated/WorkspaceReview.ts";
import type { PlanningQuestion } from "./planningQuestion.ts";

export type StatusKind = "ready" | "unknown" | "not_logged_in" | "unconfigured";

/** Mirrors `ApprovalMode` in core; wire names must match `ApprovalMode::as_str`. */
export type ApprovalMode =
  | "manual"
  | "accept_edits"
  | "plan"
  | "auto"
  | "bypass";

/**
 * Routing wire types come from Rust via ts-rs — hand-writing them here would be
 * a second source of truth that drifts silently.
 */
export type RoutingRule = RoutingRuleView;
export type { CommandView };
export type RoutingProviderModels = ProviderModelsView;
export type { RoutingView };

/** What the user clicked on an approval card. */
export type ApprovalChoice = "once" | "session" | "deny";

export const APPROVAL_MODES: {
  id: ApprovalMode;
  label: string;
  hint: string;
}[] = [
  { id: "manual", label: "Manual", hint: "Ask before every write and command" },
  {
    id: "accept_edits",
    label: "Accept edits",
    hint: "Apply file edits; still ask for commands",
  },
  // The hint says what the mode produces, not just what it forbids: it runs the
  // `plan` skill, so "read only" alone would undersell it and leave people
  // typing `/plan` inside plan mode.
  {
    id: "plan",
    label: "Plan",
    hint: "Research and write a plan — no writes, no commands",
  },
  {
    id: "auto",
    label: "Auto",
    hint: "Apply edits and safe commands; ask for the rest",
  },
  {
    id: "bypass",
    label: "Bypass permissions",
    hint: "Never ask. Use in a throwaway tree",
  },
];

/** Rust-authoritative provider row (auth + catalogue). */
export type ProviderRow = Omit<GeneratedProviderView, "statusKind"> & {
  statusKind: StatusKind;
};

export type ExternalAgentRow = ExternalAgentView;
export type ExternalAgentCheck = ExternalAgentCheckView;

export type LoginStarted = {
  browserTitle: string;
  browserBody: string;
};

export type LoginStatus = {
  state: "idle" | "running" | "exited";
  detail: string | null;
};

export type { ModelCapability };
export type WorkspaceReview = GeneratedWorkspaceReview;

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

/** Filename chips shown on a sent user bubble (UI-only; may be absent on reload). */
export type UserAttachmentChip = {
  name: string;
  kind: string;
};

export type ChatMessage =
  | {
      id: string;
      role: "user";
      text: string;
      attachments?: UserAttachmentChip[];
    }
  | {
      id: string;
      role: "assistant";
      text: string;
      thinking: string;
      tools: ToolPart[];
      error?: string;
      /** Provider to offer a Reconnect for; only set on auth failures. */
      reconnectProvider?: string;
      /** Slash command that produced this turn, if any — titles the output. */
      command?: string;
      /** Live question requested by the model; not persisted in thread history. */
      question?: PlanningQuestion;
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

export type ThreadCheckpoint = ThreadCheckpointView;

export type ThreadSummary = {
  id: string;
  createdAt: number;
  updatedAt: number;
  title?: string;
  providerId?: string;
  messageCount: number;
};

/** Sidebar grouping: one project folder + its chats. */
export type ProjectChats = {
  name: string;
  path: string;
  active: boolean;
  threads: ThreadSummary[];
};

export type PreparedAttachment = {
  id: string;
  name: string;
  path: string;
  kind: string;
  status: string;
  detail: string;
  content?: string | null;
  mediaType?: string | null;
  dataBase64?: string | null;
};

export type AttachmentInput = {
  name: string;
  detail: string;
  content?: string | null;
  status: string;
  kind?: string | null;
  mediaType?: string | null;
  dataBase64?: string | null;
};

export type ContextUsage = {
  usedTokens: number;
  windowTokens: number;
  remainingTokens: number;
  percentFull: number;
  source: string;
  systemTokens: number;
  conversationTokens: number;
  messageCount: number;
  checkpointCount: number;
  canCompact: boolean;
  autoCompactThresholdPercent: number;
  shouldAutoCompact: boolean;
};

export type UserProfile = {
  displayName: string;
  avatarDataUrl: string;
};

export type WorkspacePickResult = {
  path: string;
  sessionEnded: boolean;
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

/**
 * One day of the activity heatmap.
 *
 * `tokens` is optional on purpose: a day before token metering existed has real
 * chat counts and no spend figure, which is not the same as a metered day that
 * spent nothing. The heatmap draws those two differently.
 */
export type DayPoint = {
  date: string;
  chats: number;
  messages: number;
  tokens?: number;
  requests?: number;
};

export type ProfileStats = {
  totalChats: number;
  totalMessages: number;
  /** Lifetime, from per-provider totals that predate daily buckets. */
  totalTokens: number;
  totalRequests: number;
  peakDayTokens: number;
  longestChatSecs: number;
  currentStreakDays: number;
  longestStreakDays: number;
  firstActivity?: number;
  days: DayPoint[];
  /** ISO date metering began; earlier cells have no token figure. */
  meteringSince?: string;
};

/**
 * A provider problem found *after* the chat was already usable.
 *
 * Since opening a chat no longer waits on a live turn, verification happens in
 * the background — and a failure has to be reported without throwing the user
 * out of a session that is otherwise working.
 */
export type SessionWarning = {
  providerId: string;
  message: string;
  /** Whether signing in again is the actual fix. */
  offerReconnect: boolean;
};
