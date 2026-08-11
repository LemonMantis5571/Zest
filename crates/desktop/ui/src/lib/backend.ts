import type { UnlistenFn } from "@tauri-apps/api/event";

import * as tauriApi from "./api";
import { createFixtureBackend } from "./fixtureBackend";
import type { SkillSummary, SystemPromptInfo } from "./api";
import type {
  ApprovalChoice,
  ApprovalMode,
  CommandView,
  AttachmentInput,
  ChatEvent,
  DelegationEvent,
  DelegationJob,
  ContextUsage,
  ExternalAgentCheck,
  ExternalAgentRow,
  GitContext,
  LoginStarted,
  LoginStatus,
  PreparedAttachment,
  ProfileStats,
  ProjectChats,
  ProviderRow,
  RatesStatus,
  SessionInfo,
  SessionMeta,
  SpacesSnapshot,
  ThreadSummary,
  UsageReport,
  UsageSnapshot,
  UserProfile,
  WorkspacePickResult,
  WorkspaceReview,
} from "./types";

export type { SkillSummary, SystemPromptInfo };

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
  configureClaudeCodeProvider(input: { id: string; model: string }): Promise<void>;
  openProjectConfig(root: string): Promise<void>;
  usageSnapshot(): Promise<UsageSnapshot>;
  usageReport(days: number): Promise<UsageReport>;
  /** Open the price book in the OS editor so rates can be corrected. */
  openPricesFile(): Promise<void>;
  /**
   * Fetch the published rate table if the cached copy is due.
   *
   * Separate from `usageReport` on purpose: the report must stay instant and
   * must not fail because the network is down. Call both, and re-read the
   * report only if the rates actually moved.
   */
  refreshRates(force: boolean): Promise<RatesStatus>;
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
  }): Promise<SessionMeta>;
  listThreads(): Promise<ThreadSummary[]>;
  listSpaces(): Promise<SpacesSnapshot>;
  setActiveSpace(spaceId: string, currentWorkspacePath?: string | null): Promise<SpacesSnapshot>;
  createSpace(name: string, emoji?: string | null): Promise<SpacesSnapshot>;
  updateSpace(spaceId: string, name: string, emoji?: string | null): Promise<SpacesSnapshot>;
  deleteSpace(spaceId: string): Promise<SpacesSnapshot>;
  moveProjectToSpace(projectPath: string, spaceId: string): Promise<SpacesSnapshot>;
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
  cancelTurn(threadId?: string): Promise<void>;
  resolveApproval(
    approvalId: string,
    decision: ApprovalChoice,
    threadId?: string
  ): Promise<void>;
  resolveQuestion(
    questionId: string,
    answer: string,
    threadId?: string
  ): Promise<void>;
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
  gitContext(): Promise<GitContext>;
  verifyWorkspace(): Promise<WorkspaceReview>;
  contextUsage(): Promise<ContextUsage>;
  getUserProfile(): Promise<UserProfile>;
  setUserProfile(profile: UserProfile): Promise<UserProfile>;
  onChatEvent(handler: (event: ChatEvent) => void): Promise<UnlistenFn>;
  listDelegationJobs(): Promise<DelegationJob[]>;
  getDelegationJob(jobId: string): Promise<DelegationJob>;
  cancelDelegationJob(jobId: string): Promise<DelegationJob>;
  retryDelegationJob(jobId: string): Promise<DelegationJob>;
  applyDelegationJob(jobId: string): Promise<DelegationJob>;
  onDelegationEvent(handler: (event: DelegationEvent) => void): Promise<UnlistenFn>;
  /** Optional boot hook (fixture streams a canned turn). */
  boot?(handler: (event: ChatEvent) => void): Promise<void> | void;
};

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
    configureClaudeCodeProvider: (input) => tauriApi.configureClaudeCodeProvider(input),
    openProjectConfig: (root) => tauriApi.openProjectConfig(root),
    usageSnapshot: () => tauriApi.usageSnapshot(),
    usageReport: (days) => tauriApi.usageReport(days),
    openPricesFile: () => tauriApi.openPricesFile(),
    refreshRates: (force) => tauriApi.refreshRates(force),
    profileStats: () => tauriApi.profileStats(),
    setLocalOffset: () => tauriApi.setLocalOffset(),
    lastProvider: () => tauriApi.lastProvider(),
    startLogin: (id) => tauriApi.startLogin(id),
    loginStatus: () => tauriApi.loginStatus(),
    cancelLogin: () => tauriApi.cancelLogin(),
    startSession: (id, options) => tauriApi.startSession(id, options),
    updateSessionOptions: (options) => tauriApi.updateSessionOptions(options),
    listThreads: () => tauriApi.listThreads(),
    listSpaces: () => tauriApi.listSpaces(),
    setActiveSpace: (spaceId, currentWorkspacePath) =>
      tauriApi.setActiveSpace(spaceId, currentWorkspacePath),
    createSpace: (name, emoji) => tauriApi.createSpace(name, emoji),
    updateSpace: (spaceId, name, emoji) => tauriApi.updateSpace(spaceId, name, emoji),
    deleteSpace: (spaceId) => tauriApi.deleteSpace(spaceId),
    moveProjectToSpace: (projectPath, spaceId) =>
      tauriApi.moveProjectToSpace(projectPath, spaceId),
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
    cancelTurn: (threadId) => tauriApi.cancelTurn(threadId),
    resolveApproval: (approvalId, decision, threadId) =>
      tauriApi.resolveApproval(approvalId, decision, threadId),
    resolveQuestion: (questionId, answer, threadId) =>
      tauriApi.resolveQuestion(questionId, answer, threadId),
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
    gitContext: () => tauriApi.gitContext(),
    verifyWorkspace: () => tauriApi.verifyWorkspace(),
    contextUsage: () => tauriApi.contextUsage(),
    getUserProfile: () => tauriApi.getUserProfile(),
    setUserProfile: (profile) => tauriApi.setUserProfile(profile),
    onChatEvent: (handler) => tauriApi.onChatEvent(handler),
    listDelegationJobs: () => tauriApi.listDelegationJobs(),
    getDelegationJob: (jobId) => tauriApi.getDelegationJob(jobId),
    cancelDelegationJob: (jobId) => tauriApi.cancelDelegationJob(jobId),
    retryDelegationJob: (jobId) => tauriApi.retryDelegationJob(jobId),
    applyDelegationJob: (jobId) => tauriApi.applyDelegationJob(jobId),
    onDelegationEvent: (handler) => tauriApi.onDelegationEvent(handler),
  };
}

export function selectBackend(): DesktopBackend {
  //  is replaced with a literal at build time, so the
  // fixture import below is statically unreachable in a release and the whole
  // module is dropped. It also means  is a dev-server affordance
  // rather than something a shipped app will answer to.
  const fixture =
    import.meta.env.DEV &&
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
