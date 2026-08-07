import type { UnlistenFn } from "@tauri-apps/api/event";

import * as tauriApi from "./api";
import type { SkillSummary, SystemPromptInfo } from "./api";
import { runFixtureStream } from "./fixture";
import { safeMarkdownFilename } from "./markdownExport";
import {
  CODEX_MODELS,
  DEFAULT_CODEX_MODEL,
  DEFAULT_EFFORT,
} from "./models";
import type {
  ApprovalChoice,
  ApprovalMode,
  CommandView,
  AttachmentInput,
  ChatEvent,
  ChatMessage,
  ContextUsage,
  ExternalAgentCheck,
  ExternalAgentRow,
  LoginStarted,
  LoginStatus,
  PreparedAttachment,
  ProfileStats,
  ProjectChats,
  ProviderRow,
  SessionInfo,
  ThreadSummary,
  UsageSnapshot,
  UserProfile,
  WorkspacePickResult,
  WorkspaceReview,
} from "./types";

export type { SkillSummary, SystemPromptInfo };

const FIXTURE_MODELS = CODEX_MODELS.map((m) => ({
  id: m.id,
  efforts: ["low", "medium", "high", "xhigh", "max"],
  contextWindow: 256000,
  supportsTools: true,
  supportsVision: false,
}));

/** Desktop I/O surface used by App — Tauri in production, fixture offline. */
export type DesktopBackend = {
  readonly mode: "tauri" | "fixture";
  listProviders(): Promise<ProviderRow[]>;
  listExternalAgents(): Promise<ExternalAgentRow[]>;
  setExternalAgent(id: string, enabled: boolean): Promise<void>;
  setExternalAgentMcp(id: string, enabled: boolean): Promise<void>;
  setExternalAgentModel(id: string, model: string | null): Promise<void>;
  checkExternalAgent(id: string): Promise<ExternalAgentCheck>;
  setProviderKey(id: string, key: string): Promise<void>;
  deleteProviderKey(id: string): Promise<void>;
  providerKeyPresent(id: string): Promise<boolean>;
  configureApiProvider(input: {
    id: string;
    baseUrl: string;
    model: string;
    models: string[];
    credential: string;
    key: string;
  }): Promise<void>;
  configureAnthropicProvider(input: {
    id: string;
    model: string;
    credential: string;
    key: string;
  }): Promise<void>;
  openProjectConfig(root: string): Promise<void>;
  usageSnapshot(): Promise<UsageSnapshot>;
  profileStats(): Promise<ProfileStats>;
  /** Hand core this machine's UTC offset so day boundaries match the clock. */
  setLocalOffset(): Promise<void>;
  lastProvider(): Promise<string | null>;
  startLogin(id: string): Promise<LoginStarted>;
  loginStatus(): Promise<LoginStatus>;
  cancelLogin(): Promise<void>;
  startSession(
    id: string,
    options?: { model?: string; effort?: string }
  ): Promise<SessionInfo>;
  updateSessionOptions(options: {
    model?: string;
    effort?: string;
  }): Promise<SessionInfo>;
  listThreads(): Promise<ThreadSummary[]>;
  listChatProjects(): Promise<ProjectChats[]>;
  openProjectChat(options: {
    root: string;
    threadId?: string | null;
    newThread?: boolean;
    providerId?: string | null;
    copyThread?: boolean;
  }): Promise<SessionInfo>;
  loadThread(id: string): Promise<SessionInfo>;
  newThread(): Promise<SessionInfo>;
  sessionInfo(): Promise<SessionInfo | null>;
  forkThread(): Promise<SessionInfo>;
  rewindThread(checkpointId: string): Promise<SessionInfo>;
  editMessage(messageId: string): Promise<SessionInfo>;
  compactContext(): Promise<ContextUsage>;
  deleteThread(id: string, projectPath?: string | null): Promise<SessionInfo>;
  setThreadPinned(
    id: string,
    projectPath: string | null | undefined,
    pinned: boolean
  ): Promise<void>;
  sendMessage(text: string, attachments?: AttachmentInput[]): Promise<void>;
  saveMarkdown(suggestedName: string, markdown: string): Promise<string | null>;
  cancelTurn(): Promise<void>;
  resolveApproval(approvalId: string, decision: ApprovalChoice): Promise<void>;
  resolveQuestion(questionId: string, answer: string): Promise<void>;
  setApprovalMode(mode: ApprovalMode): Promise<string>;
  approvalMode(): Promise<string>;
  verifyProvider(id: string): Promise<void>;
  listCommands(): Promise<CommandView[]>;
  endSession(): Promise<void>;
  getSystemPrompt(): Promise<SystemPromptInfo>;
  setSystemPrompt(custom: string): Promise<SystemPromptInfo>;
  listSkills(): Promise<SkillSummary[]>;
  getWorkspaceFolder(): Promise<string>;
  pickWorkspaceFolder(): Promise<WorkspacePickResult | null>;
  pickFiles(): Promise<PreparedAttachment[]>;
  preparePastedImage(options: {
    dataBase64: string;
    mediaType: string;
    name?: string;
  }): Promise<PreparedAttachment>;
  gitBranch(): Promise<string | null>;
  verifyWorkspace(): Promise<WorkspaceReview>;
  contextUsage(): Promise<ContextUsage>;
  getUserProfile(): Promise<UserProfile>;
  setUserProfile(profile: UserProfile): Promise<UserProfile>;
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
  checkpoints: [],
  messages: [],
};

function notAvailable(op: string): never {
  throw new Error(`fixture backend: ${op} is not available`);
}

export function createTauriBackend(): DesktopBackend {
  return {
    mode: "tauri",
    listProviders: () => tauriApi.listProviders(),
    listExternalAgents: () => tauriApi.listExternalAgents(),
    setExternalAgent: (id, enabled) => tauriApi.setExternalAgent(id, enabled),
    setExternalAgentMcp: (id, enabled) => tauriApi.setExternalAgentMcp(id, enabled),
    setExternalAgentModel: (id, model) => tauriApi.setExternalAgentModel(id, model),
    checkExternalAgent: (id) => tauriApi.checkExternalAgent(id),
    setProviderKey: (id, key) => tauriApi.setProviderKey(id, key),
    deleteProviderKey: (id) => tauriApi.deleteProviderKey(id),
    providerKeyPresent: (id) => tauriApi.providerKeyPresent(id),
    configureApiProvider: (input) => tauriApi.configureApiProvider(input),
    configureAnthropicProvider: (input) => tauriApi.configureAnthropicProvider(input),
    openProjectConfig: (root) => tauriApi.openProjectConfig(root),
    usageSnapshot: () => tauriApi.usageSnapshot(),
    profileStats: () => tauriApi.profileStats(),
    setLocalOffset: () => tauriApi.setLocalOffset(),
    lastProvider: () => tauriApi.lastProvider(),
    startLogin: (id) => tauriApi.startLogin(id),
    loginStatus: () => tauriApi.loginStatus(),
    cancelLogin: () => tauriApi.cancelLogin(),
    startSession: (id, options) => tauriApi.startSession(id, options),
    updateSessionOptions: (options) => tauriApi.updateSessionOptions(options),
    listThreads: () => tauriApi.listThreads(),
    listChatProjects: () => tauriApi.listChatProjects(),
    openProjectChat: (options) => tauriApi.openProjectChat(options),
    loadThread: (id) => tauriApi.loadThread(id),
    newThread: () => tauriApi.newThread(),
    sessionInfo: () => tauriApi.sessionInfo(),
    forkThread: () => tauriApi.forkThread(),
    rewindThread: (checkpointId) => tauriApi.rewindThread(checkpointId),
    editMessage: (messageId) => tauriApi.editMessage(messageId),
    compactContext: () => tauriApi.compactContext(),
    deleteThread: (id, projectPath) => tauriApi.deleteThread(id, projectPath),
    setThreadPinned: (id, projectPath, pinned) =>
      tauriApi.setThreadPinned(id, projectPath, pinned),
    sendMessage: (text, attachments) => tauriApi.sendMessage(text, attachments),
    saveMarkdown: (suggestedName, markdown) =>
      tauriApi.saveMarkdown(suggestedName, markdown),
    cancelTurn: () => tauriApi.cancelTurn(),
    resolveApproval: (approvalId, decision) =>
      tauriApi.resolveApproval(approvalId, decision),
    resolveQuestion: (questionId, answer) =>
      tauriApi.resolveQuestion(questionId, answer),
    setApprovalMode: (mode) => tauriApi.setApprovalMode(mode),
    approvalMode: () => tauriApi.approvalMode(),
    verifyProvider: (id) => tauriApi.verifyProvider(id),
    listCommands: () => tauriApi.listCommands(),
    endSession: () => tauriApi.endSession(),
    getSystemPrompt: () => tauriApi.getSystemPrompt(),
    setSystemPrompt: (custom) => tauriApi.setSystemPrompt(custom),
    listSkills: () => tauriApi.listSkills(),
    getWorkspaceFolder: () => tauriApi.getWorkspaceFolder(),
    pickWorkspaceFolder: () => tauriApi.pickWorkspaceFolder(),
    pickFiles: () => tauriApi.pickFiles(),
    preparePastedImage: (options) => tauriApi.preparePastedImage(options),
    gitBranch: () => tauriApi.gitBranch(),
    verifyWorkspace: () => tauriApi.verifyWorkspace(),
    contextUsage: () => tauriApi.contextUsage(),
    getUserProfile: () => tauriApi.getUserProfile(),
    setUserProfile: (profile) => tauriApi.setUserProfile(profile),
    onChatEvent: (handler) => tauriApi.onChatEvent(handler),
  };
}

export function createFixtureBackend(): DesktopBackend {
  let session: SessionInfo = { ...FIXTURE_SESSION, messages: [] };
  let chatHandler: ((event: ChatEvent) => void) | null = null;
  let workspace = ".";
  let fixturePinned = false;
  const enabledExternalAgents = new Set<string>();
  const fixtureMcpAgents = new Set<string>();
  const fixtureExternalModels = new Map<string, string>();

  const fixtureExternalModelOptions: Record<string, string[]> = {
    claude: ["sonnet", "opus"],
    gemini: [
      "auto",
      "gemini-3-pro-preview",
      "gemini-3-flash-preview",
      "gemini-2.5-pro",
      "gemini-2.5-flash",
    ],
  };

  return {
    mode: "fixture",
    async listExternalAgents() {
      return [
        {
          id: "claude",
          label: "Claude Code",
          scope: "Project zest.toml",
          mode: "Headless CLI",
          workspace: "Isolated worktree",
          statusLabel: enabledExternalAgents.has("claude") ? "Delegation enabled" : "Delegation off",
          detail: enabledExternalAgents.has("claude")
            ? "Delegates through your Claude Code CLI session."
            : "Enable delegation to let Zest send bounded tasks to Claude Code.",
          configured: enabledExternalAgents.has("claude"),
          mcpAllowed: enabledExternalAgents.has("claude") && fixtureMcpAgents.has("claude"),
          model: fixtureExternalModels.get("claude") ?? "",
          models: fixtureExternalModelOptions.claude,
          preset: true,
        },
        {
          id: "gemini",
          label: "Gemini CLI",
          scope: "Project zest.toml",
          mode: "CLI via ACP",
          workspace: "Isolated worktree",
          statusLabel: enabledExternalAgents.has("gemini") ? "Delegation enabled" : "Delegation off",
          detail: enabledExternalAgents.has("gemini")
            ? "Delegates through your Gemini CLI session."
            : "Enable delegation to let Zest send bounded tasks to Gemini CLI.",
          configured: enabledExternalAgents.has("gemini"),
          mcpAllowed: enabledExternalAgents.has("gemini") && fixtureMcpAgents.has("gemini"),
          model: fixtureExternalModels.get("gemini") ?? "",
          models: fixtureExternalModelOptions.gemini,
          preset: true,
        },
      ];
    },
    async setExternalAgent(id, enabled) {
      if (enabled) enabledExternalAgents.add(id);
      else {
        enabledExternalAgents.delete(id);
        fixtureMcpAgents.delete(id);
        fixtureExternalModels.delete(id);
      }
    },
    async setExternalAgentMcp(id, enabled) {
      if (enabled) fixtureMcpAgents.add(id);
      else fixtureMcpAgents.delete(id);
    },
    async setExternalAgentModel(id, model) {
      if (model?.trim()) fixtureExternalModels.set(id, model.trim());
      else fixtureExternalModels.delete(id);
    },
    async checkExternalAgent() {
      return {
        available: false,
        authenticated: null,
        detail: "CLI checks are unavailable in the fixture.",
      };
    },
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
        externalWorkers: [],
      };
    },
    async setProviderKey() {
      notAvailable("setProviderKey");
    },
    async deleteProviderKey() {
      notAvailable("deleteProviderKey");
    },
    async providerKeyPresent() {
      return false;
    },
    async configureApiProvider() {
      notAvailable("configureApiProvider");
    },
    async configureAnthropicProvider() {
      notAvailable("configureAnthropicProvider");
    },
    async openProjectConfig() {
      notAvailable("openProjectConfig");
    },
    async profileStats() {
      // Enough shape to exercise the heatmap offline: a long run of days, a
      // gap, and a metering start part-way through so the "no token data"
      // rendering is visible rather than only reachable on a real install.
      const day = 86_400;
      const today = Math.floor(Date.now() / 1000);
      const iso = (offsetDays: number) =>
        new Date((today - offsetDays * day) * 1000).toISOString().slice(0, 10);

      const days = [];
      for (let back = 180; back >= 0; back--) {
        // A believable rhythm rather than noise: quiet weekends, busy weekdays.
        const weekday = new Date((today - back * day) * 1000).getDay();
        const busy = weekday !== 0 && weekday !== 6;
        const chats = busy ? (back % 5 === 0 ? 0 : 1 + (back % 4)) : back % 3 === 0 ? 1 : 0;
        if (chats === 0 && back % 7 !== 0) continue;
        days.push({
          date: iso(back),
          chats,
          messages: chats * (4 + (back % 9)),
          // Metering began 90 days ago; earlier cells carry no token figure.
          ...(back <= 90
            ? { tokens: chats * 12_000 + (back % 11) * 900, requests: chats * 3 }
            : {}),
        });
      }

      return {
        totalChats: days.reduce((sum, d) => sum + d.chats, 0),
        totalMessages: days.reduce((sum, d) => sum + d.messages, 0),
        totalTokens: 5_252_800_000,
        totalRequests: 12_480,
        peakDayTokens: 338_300_000,
        longestChatSecs: 4_380,
        currentStreakDays: 28,
        longestStreakDays: 71,
        firstActivity: today - 180 * day,
        days,
        meteringSince: iso(90),
      };
    },
    async setLocalOffset() {
      /* fixture: no core to inform */
    },
    async lastProvider() {
      return "fixture";
    },
    async startLogin() {
      return notAvailable("startLogin");
    },
    async loginStatus() {
      return { state: "idle", detail: null };
    },
    async cancelLogin() {
      /* fixture: no child process to stop */
    },
    async startSession() {
      fixturePinned = false;
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
          updatedAt: Math.floor(Date.now() / 1000),
          title: "Fixture",
          pinned: fixturePinned,
          messageCount: session.messages.length,
        },
      ];
    },
    async listChatProjects() {
      const threads = await this.listThreads();
      return [
        {
          name: workspace.split(/[/\\]/).filter(Boolean).pop() || "fixture",
          path: workspace,
          active: true,
          threads,
        },
      ];
    },
    async openProjectChat(options) {
      workspace = options.root;
      if (options.newThread) fixturePinned = false;
      session = {
        ...session,
        root: workspace,
        messages: options.newThread ? [] : session.messages,
        threadId: options.newThread
          ? `fixture-${crypto.randomUUID()}`
          : options.threadId || session.threadId,
      };
      return { ...session };
    },
    async loadThread(id: string) {
      if (id !== session.threadId) {
        throw new Error(`fixture: unknown thread ${id}`);
      }
      return { ...session };
    },
    async newThread() {
      fixturePinned = false;
      session = {
        ...FIXTURE_SESSION,
        root: workspace,
        threadId: `fixture-${crypto.randomUUID()}`,
        messages: [],
      };
      return { ...session };
    },
    async sessionInfo() {
      return { ...session };
    },
    async forkThread() {
      fixturePinned = false;
      session = {
        ...session,
        threadId: `fixture-${crypto.randomUUID()}`,
        checkpoints: [],
      };
      return { ...session };
    },
    async rewindThread() {
      return { ...session };
    },
    async editMessage(messageId: string) {
      const index = session.messages.findIndex((message) => message.id === messageId);
      if (index < 0 || session.messages[index]?.role !== "user") {
        throw new Error("fixture: user message not found");
      }
      session = { ...session, messages: session.messages.slice(0, index) };
      return { ...session };
    },
    async compactContext() {
      return this.contextUsage();
    },
    async deleteThread(id: string) {
      if (id === session.threadId) {
        return this.newThread();
      }
      return { ...session };
    },
    async setThreadPinned(_id, _projectPath, pinned) {
      fixturePinned = pinned;
    },
    async sendMessage(text: string, attachments?: AttachmentInput[]) {
      if (!chatHandler) return;
      const turnId = `turn-${crypto.randomUUID()}`;
      const userId = `user-${crypto.randomUUID()}`;
      const assistantId = `assistant-${crypto.randomUUID()}`;
      const id = {
        session_id: session.sessionId,
        thread_id: session.threadId,
        turn_id: turnId,
      };
      let display = text.trim();
      if (attachments?.length) {
        const lines = attachments.map((a) => `Attached: ${a.name} (${a.detail})`);
        display = display ? `${display}\n\n${lines.join("\n")}` : lines.join("\n");
      }
      const fixtureAssistantText = `Fixture echo: ${text.trim() || "(attachment)"}`;
      const fixtureUser: ChatMessage = {
        id: userId,
        role: "user",
        text: display,
        attachments: attachments?.length
          ? attachments.map((attachment) => ({
              name: attachment.name,
              kind: attachment.kind ?? "file",
            }))
          : undefined,
      };
      const fixtureAssistant: ChatMessage = {
        id: assistantId,
        role: "assistant",
        text: fixtureAssistantText,
        thinking: "",
        tools: [],
        streaming: false,
      };
      session = {
        ...session,
        messages: [...session.messages, fixtureUser, fixtureAssistant],
      };
      chatHandler({ kind: "user", ...id, message_id: userId, text: display });
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
        text: text.trim() || "(attachment)",
      });
      chatHandler({ kind: "done", ...id, message_id: assistantId });
    },
    async saveMarkdown(suggestedName, markdown) {
      const filename = safeMarkdownFilename(suggestedName, "response");
      const blob = new Blob([markdown], { type: "text/markdown;charset=utf-8" });
      const url = URL.createObjectURL(blob);
      const link = document.createElement("a");
      link.href = url;
      link.download = filename;
      link.click();
      URL.revokeObjectURL(url);
      return filename;
    },
    async cancelTurn() {
      /* no-op offline */
    },
    async resolveApproval() {
      throw new Error("fixture: no pending approvals");
    },
    async resolveQuestion() {
      throw new Error("fixture: no pending questions");
    },
    async setApprovalMode(mode: ApprovalMode) {
      return mode;
    },
    async approvalMode() {
      return "auto";
    },
    async verifyProvider() {
      /* fixture: nothing to verify */
    },
    async listCommands() {
      return [];
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
    async getWorkspaceFolder() {
      return workspace;
    },
    async pickWorkspaceFolder() {
      workspace = `fixture/project-${crypto.randomUUID().slice(0, 8)}`;
      session = { ...session, root: workspace, messages: [] };
      return { path: workspace, sessionEnded: false };
    },
    async pickFiles() {
      return [
        {
          id: `att-${crypto.randomUUID()}`,
          name: "sample.pdf",
          path: `${workspace}/sample.pdf`,
          kind: "pdf",
          status: "done",
          detail: "TextBased, 1 pages",
          content: "# Fixture PDF\n\nExtracted markdown from pdf-inspector path.",
        },
      ];
    },
    async preparePastedImage(options) {
      return {
        id: `att-${crypto.randomUUID()}`,
        name: options.name ?? "paste.png",
        path: "clipboard",
        kind: "image",
        status: "done",
        detail: "pasted",
        mediaType: options.mediaType,
        dataBase64: options.dataBase64.includes(",")
          ? options.dataBase64.split(",").pop()!
          : options.dataBase64,
      };
    },
    async gitBranch() {
      return "master";
    },
    async verifyWorkspace() {
      return {
        summary: "Fixture workspace is ready.",
        repository: "git",
        changedFiles: ["src/example.ts"],
        changedFileCount: 1,
        patchCheck: "clean",
      };
    },
    async contextUsage() {
      return {
        usedTokens: 12000,
        windowTokens: 256000,
        remainingTokens: 244000,
        percentFull: 4.7,
        source: "estimate",
        systemTokens: 3200,
        conversationTokens: 8800,
        messageCount: session.messages.length,
        checkpointCount: session.checkpoints.length,
        canCompact: session.messages.length >= 4,
        autoCompactThresholdPercent: 80,
        shouldAutoCompact: false,
      };
    },
    async getUserProfile() {
      return { displayName: "Fixture", avatarDataUrl: "" };
    },
    async setUserProfile(profile) {
      return profile;
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
