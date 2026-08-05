import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type {
  ApprovalChoice,
  ApprovalMode,
  CommandView,
  AttachmentInput,
  ChatEvent,
  ContextUsage,
  ExternalAgentCheck,
  ExternalAgentRow,
  LoginStarted,
  LoginStatus,
  PreparedAttachment,
  ProfileStats,
  ProviderRow,
  SessionInfo,
  ProjectChats,
  ThreadSummary,
  UsageSnapshot,
  UserProfile,
  WorkspacePickResult,
  WorkspaceReview,
} from "./types";

export function listProviders() {
  return invoke<ProviderRow[]>("list_providers");
}

export function listExternalAgents() {
  return invoke<ExternalAgentRow[]>("list_external_agents");
}

export function setExternalAgent(id: string, enabled: boolean) {
  return invoke<void>("set_external_agent", { id, enabled });
}

export function checkExternalAgent(id: string) {
  return invoke<ExternalAgentCheck>("check_external_agent", { id });
}

export function setProviderKey(id: string, key: string) {
  return invoke<void>("set_provider_key", { id, key });
}

export function deleteProviderKey(id: string) {
  return invoke<void>("delete_provider_key", { id });
}

export function providerKeyPresent(id: string) {
  return invoke<boolean>("provider_key_present", { id });
}

export function configureApiProvider(input: {
  id: string;
  baseUrl: string;
  model: string;
  models: string[];
  credential: string;
  key: string;
}) {
  return invoke<void>("configure_api_provider", input);
}

export function usageSnapshot() {
  return invoke<UsageSnapshot>("usage_snapshot");
}

export function profileStats() {
  return invoke<ProfileStats>("profile_stats");
}

/**
 * Hand core this machine's UTC offset.
 *
 * The webview is the only part of Zest that knows the timezone, and every day
 * boundary depends on it. `getTimezoneOffset` reports minutes *behind* UTC, so
 * the sign is flipped to the usual "minutes east" convention.
 */
export function setLocalOffset(minutes = -new Date().getTimezoneOffset()) {
  return invoke<void>("set_local_offset", { minutes });
}

export function lastProvider() {
  return invoke<string | null>("last_provider");
}

export function startLogin(id: string) {
  return invoke<LoginStarted>("start_login", { id });
}

export function loginStatus() {
  return invoke<LoginStatus>("login_status");
}

export function cancelLogin() {
  return invoke<void>("cancel_login");
}

export function startSession(
  id: string,
  options?: { model?: string; effort?: string }
) {
  return invoke<SessionInfo>("start_session", {
    id,
    model: options?.model ?? null,
    effort: options?.effort ?? null,
  });
}

export function updateSessionOptions(options: {
  model?: string;
  effort?: string;
}) {
  return invoke<SessionInfo>("update_session_options", {
    model: options.model ?? null,
    effort: options.effort ?? null,
  });
}

export function listThreads() {
  return invoke<ThreadSummary[]>("list_threads");
}

export function listChatProjects() {
  return invoke<ProjectChats[]>("list_chat_projects");
}

export function openProjectChat(options: {
  root: string;
  threadId?: string | null;
  newThread?: boolean;
}) {
  return invoke<SessionInfo>("open_project_chat", {
    root: options.root,
    threadId: options.threadId ?? null,
    newThread: options.newThread ?? null,
  });
}

export function loadThread(id: string) {
  return invoke<SessionInfo>("load_thread", { id });
}

export function newThread() {
  return invoke<SessionInfo>("new_thread");
}

export function sessionInfo() {
  return invoke<SessionInfo | null>("session_info");
}

export function forkThread() {
  return invoke<SessionInfo>("fork_thread");
}

export function rewindThread(checkpointId: string) {
  return invoke<SessionInfo>("rewind_thread", { checkpointId });
}

export function compactContext() {
  return invoke<ContextUsage>("compact_context");
}

export function deleteThread(id: string, projectPath?: string | null) {
  return invoke<SessionInfo>("delete_thread", {
    id,
    projectPath: projectPath ?? null,
  });
}

export function sendMessage(text: string, attachments?: AttachmentInput[]) {
  return invoke<void>("send_message", {
    text,
    attachments: attachments ?? null,
  });
}

export function saveMarkdown(suggestedName: string, markdown: string) {
  return invoke<string | null>("save_markdown", {
    suggestedName,
    markdown,
  });
}

export function getWorkspaceFolder() {
  return invoke<string>("get_workspace_folder");
}

export function pickWorkspaceFolder() {
  return invoke<WorkspacePickResult | null>("pick_workspace_folder");
}

export function pickFiles() {
  return invoke<PreparedAttachment[]>("pick_files");
}

export function preparePastedImage(options: {
  dataBase64: string;
  mediaType: string;
  name?: string;
}) {
  return invoke<PreparedAttachment>("prepare_pasted_image", {
    dataBase64: options.dataBase64,
    mediaType: options.mediaType,
    name: options.name ?? null,
  });
}

export function gitBranch() {
  return invoke<string | null>("git_branch");
}

export function verifyWorkspace() {
  return invoke<WorkspaceReview>("verify_workspace");
}

export function contextUsage() {
  return invoke<ContextUsage>("context_usage");
}

export function getUserProfile() {
  return invoke<UserProfile>("get_user_profile");
}

export function setUserProfile(profile: UserProfile) {
  return invoke<UserProfile>("set_user_profile", { profile });
}

export function cancelTurn() {
  return invoke<void>("cancel_turn");
}

export function resolveApproval(approvalId: string, decision: ApprovalChoice) {
  return invoke<void>("resolve_approval", { approvalId, decision });
}

export function resolveQuestion(questionId: string, answer: string) {
  return invoke<void>("resolve_question", { questionId, answer });
}

export type ReadingDiffView = {
  diff: string;
  summary: string;
  removedLines: number;
  foldedLines: number;
};

export function generateReadingDiff(diff: string) {
  return invoke<ReadingDiffView>("generate_reading_diff", { diff });
}

export function setApprovalMode(mode: ApprovalMode) {
  return invoke<string>("set_approval_mode", { mode });
}

export function verifyProvider(id: string) {
  return invoke<void>("verify_provider", { id });
}

export function listCommands() {
  return invoke<CommandView[]>("list_commands");
}

export function approvalMode() {
  return invoke<string>("approval_mode");
}

export function endSession() {
  return invoke<void>("end_session");
}

export function onChatEvent(handler: (event: ChatEvent) => void): Promise<UnlistenFn> {
  return listen<ChatEvent>("chat-event", (event) => handler(event.payload));
}

export type SystemPromptInfo = {
  base: string;
  custom: string;
  composedPreview: string;
  customPath: string;
};

export type SkillSummary = {
  name: string;
  description: string;
  source: "user" | "project";
  path: string;
  inlined: boolean;
};

export function getSystemPrompt() {
  return invoke<SystemPromptInfo>("get_system_prompt");
}

export function setSystemPrompt(custom: string) {
  return invoke<SystemPromptInfo>("set_system_prompt", { custom });
}

export function listSkills() {
  return invoke<SkillSummary[]>("list_skills");
}
