import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type {
  ChatEvent,
  LoginStarted,
  ProviderRow,
  SessionInfo,
  ThreadSummary,
  UsageSnapshot,
} from "./types";

export function listProviders() {
  return invoke<ProviderRow[]>("list_providers");
}

export function usageSnapshot() {
  return invoke<UsageSnapshot>("usage_snapshot");
}

export function lastProvider() {
  return invoke<string | null>("last_provider");
}

export function startLogin(id: string) {
  return invoke<LoginStarted>("start_login", { id });
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

export function loadThread(id: string) {
  return invoke<SessionInfo>("load_thread", { id });
}

export function newThread() {
  return invoke<SessionInfo>("new_thread");
}

export function sendMessage(text: string) {
  return invoke<void>("send_message", { text });
}

export function cancelTurn() {
  return invoke<void>("cancel_turn");
}

export function resolveApproval(approvalId: string, allow: boolean) {
  return invoke<void>("resolve_approval", { approvalId, allow });
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
