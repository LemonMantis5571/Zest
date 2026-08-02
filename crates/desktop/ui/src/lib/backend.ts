import type { UnlistenFn } from "@tauri-apps/api/event";

import * as tauriApi from "./api";
import type { SkillSummary, SystemPromptInfo } from "./api";
import { runFixtureStream } from "./fixture";
import {
  CODEX_MODELS,
  DEFAULT_CODEX_MODEL,
  DEFAULT_EFFORT,
} from "./models";
import type {
  ChatEvent,
  LoginStarted,
  ProviderRow,
  SessionInfo,
  ThreadSummary,
  UsageSnapshot,
} from "./types";

export type { SkillSummary, SystemPromptInfo };

const FIXTURE_MODELS = CODEX_MODELS.map((m) => ({
  id: m.id,
  efforts: ["low", "medium", "high", "xhigh", "max"],
}));

/** Desktop I/O surface used by App — Tauri in production, fixture offline. */
export type DesktopBackend = {
  readonly mode: "tauri" | "fixture";
  listProviders(): Promise<ProviderRow[]>;
  usageSnapshot(): Promise<UsageSnapshot>;
  lastProvider(): Promise<string | null>;
  startLogin(id: string): Promise<LoginStarted>;
  startSession(
    id: string,
    options?: { model?: string; effort?: string }
  ): Promise<SessionInfo>;
  updateSessionOptions(options: {
    model?: string;
    effort?: string;
  }): Promise<SessionInfo>;
  listThreads(): Promise<ThreadSummary[]>;
  loadThread(id: string): Promise<SessionInfo>;
  newThread(): Promise<SessionInfo>;
  sendMessage(text: string): Promise<void>;
  cancelTurn(): Promise<void>;
  resolveApproval(approvalId: string, allow: boolean): Promise<void>;
  endSession(): Promise<void>;
  getSystemPrompt(): Promise<SystemPromptInfo>;
  setSystemPrompt(custom: string): Promise<SystemPromptInfo>;
  listSkills(): Promise<SkillSummary[]>;
  onChatEvent(handler: (event: ChatEvent) => void): Promise<UnlistenFn>;
  /** Optional boot hook (fixture streams a canned turn). */
  boot?(handler: (event: ChatEvent) => void): Promise<void> | void;
};

const FIXTURE_SESSION: SessionInfo = {
  sessionId: "session-fixture",
  provider: "fixture",
  label: "Fixture",
  model: DEFAULT_CODEX_MODEL,
  effort: DEFAULT_EFFORT,
  root: ".",
  threadId: "fixture",
  defaultModel: DEFAULT_CODEX_MODEL,
  models: FIXTURE_MODELS,
  messages: [],
};

function notAvailable(op: string): never {
  throw new Error(`fixture backend: ${op} is not available`);
}

export function createTauriBackend(): DesktopBackend {
  return {
    mode: "tauri",
    listProviders: () => tauriApi.listProviders(),
    usageSnapshot: () => tauriApi.usageSnapshot(),
    lastProvider: () => tauriApi.lastProvider(),
    startLogin: (id) => tauriApi.startLogin(id),
    startSession: (id, options) => tauriApi.startSession(id, options),
    updateSessionOptions: (options) => tauriApi.updateSessionOptions(options),
    listThreads: () => tauriApi.listThreads(),
    loadThread: (id) => tauriApi.loadThread(id),
    newThread: () => tauriApi.newThread(),
    sendMessage: (text) => tauriApi.sendMessage(text),
    cancelTurn: () => tauriApi.cancelTurn(),
    resolveApproval: (approvalId, allow) =>
      tauriApi.resolveApproval(approvalId, allow),
    endSession: () => tauriApi.endSession(),
    getSystemPrompt: () => tauriApi.getSystemPrompt(),
    setSystemPrompt: (custom) => tauriApi.setSystemPrompt(custom),
    listSkills: () => tauriApi.listSkills(),
    onChatEvent: (handler) => tauriApi.onChatEvent(handler),
  };
}

export function createFixtureBackend(): DesktopBackend {
  let session: SessionInfo = { ...FIXTURE_SESSION, messages: [] };
  let chatHandler: ((event: ChatEvent) => void) | null = null;

  return {
    mode: "fixture",
    async listProviders() {
      return [
        {
          id: "fixture",
          label: "Fixture",
          method: "offline",
          statusKind: "ready",
          statusLabel: "Ready",
          detail: "Deterministic UI stream",
          selectable: true,
          canConnect: false,
          configured: true,
          defaultModel: DEFAULT_CODEX_MODEL,
          models: FIXTURE_MODELS,
        },
      ];
    },
    async usageSnapshot() {
      return {
        providers: [
          {
            providerId: "fixture",
            measured: {
              label: "Measured by Zest",
              requests: 0,
              inputTokens: 0,
              outputTokens: 0,
              cacheWriteTokens: 0,
              cacheReadTokens: 0,
              totalTokens: 0,
            },
            headroom: { kind: "not_reported", label: "Not reported" },
          },
        ],
      };
    },
    async lastProvider() {
      return "fixture";
    },
    async startLogin() {
      return notAvailable("startLogin");
    },
    async startSession() {
      session = { ...FIXTURE_SESSION, messages: [] };
      return { ...session };
    },
    async updateSessionOptions(options) {
      session = {
        ...session,
        model: options.model ?? session.model,
        effort: options.effort ?? session.effort,
      };
      return { ...session, messages: [] };
    },
    async listThreads() {
      return [
        {
          id: session.threadId,
          createdAt: 0,
          updatedAt: 0,
          title: "Fixture",
          messageCount: session.messages.length,
        },
      ];
    },
    async loadThread(id: string) {
      if (id !== session.threadId) {
        throw new Error(`fixture: unknown thread ${id}`);
      }
      return { ...session };
    },
    async newThread() {
      session = {
        ...FIXTURE_SESSION,
        threadId: `fixture-${crypto.randomUUID()}`,
        messages: [],
      };
      return { ...session };
    },
    async sendMessage(text: string) {
      if (!chatHandler) return;
      const turnId = `turn-${crypto.randomUUID()}`;
      const userId = `user-${crypto.randomUUID()}`;
      const assistantId = `assistant-${crypto.randomUUID()}`;
      const id = {
        session_id: session.sessionId,
        thread_id: session.threadId,
        turn_id: turnId,
      };
      chatHandler({ kind: "user", ...id, message_id: userId, text });
      chatHandler({
        kind: "assistant_start",
        ...id,
        message_id: assistantId,
      });
      chatHandler({
        kind: "text_delta",
        ...id,
        message_id: assistantId,
        text: "Fixture echo: ",
      });
      chatHandler({
        kind: "text_delta",
        ...id,
        message_id: assistantId,
        text,
      });
      chatHandler({ kind: "done", ...id, message_id: assistantId });
    },
    async cancelTurn() {
      /* no-op offline */
    },
    async resolveApproval() {
      throw new Error("fixture: no pending approvals");
    },
    async endSession() {
      /* no-op */
    },
    async getSystemPrompt() {
      return {
        base: "Fixture base system prompt.",
        custom: "",
        composedPreview: "Fixture base system prompt.",
        customPath: ".zest/system.md",
      };
    },
    async setSystemPrompt(custom: string) {
      return {
        base: "Fixture base system prompt.",
        custom,
        composedPreview: custom
          ? `Fixture base system prompt.\n\n# Project instructions\n\n${custom}`
          : "Fixture base system prompt.",
        customPath: ".zest/system.md",
      };
    },
    async listSkills() {
      return [];
    },
    async onChatEvent(handler) {
      chatHandler = handler;
      return () => {
        if (chatHandler === handler) chatHandler = null;
      };
    },
    async boot(handler) {
      chatHandler = handler;
      await runFixtureStream(handler);
    },
  };
}

export function selectBackend(): DesktopBackend {
  const fixture =
    typeof window !== "undefined" &&
    new URLSearchParams(window.location.search).has("fixture");
  return fixture ? createFixtureBackend() : createTauriBackend();
}

let sharedBackend: DesktopBackend | null = null;

/** Process-wide backend (fixture keeps in-memory session state). */
export function getBackend(): DesktopBackend {
  if (!sharedBackend) sharedBackend = selectBackend();
  return sharedBackend;
}
