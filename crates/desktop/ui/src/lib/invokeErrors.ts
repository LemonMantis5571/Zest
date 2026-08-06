type DesktopErrorPayload = {
  code?: unknown;
  message?: unknown;
  details?: unknown;
};

export type ConversationProviderChoice = {
  id: string;
  label: string;
  model: string;
};

export type ConversationRecovery =
  | {
      kind: "unknown_owner";
      threadId: string;
      providers: ConversationProviderChoice[];
    }
  | {
      kind: "owner_unavailable";
      threadId: string;
      providerId: string;
      providerLabel: string;
      configured: boolean;
      providers: ConversationProviderChoice[];
    };

function parseDesktopError(error: unknown): DesktopErrorPayload | null {
  const raw = String(error);
  try {
    const start = raw.indexOf("{");
    const end = raw.lastIndexOf("}");
    if (start >= 0 && end > start) {
      const parsed = JSON.parse(raw.slice(start, end + 1)) as DesktopErrorPayload;
      if (parsed && typeof parsed === "object") return parsed;
    }
  } catch {
    // Keep the raw value available for internal classification only.
  }
  return null;
}

export function rawInvokeError(error: unknown): string {
  const raw = String(error);
  const parsed = parseDesktopError(error);
  if (typeof parsed?.message === "string" && parsed.message) {
    return typeof parsed.code === "string" && parsed.code
      ? parsed.code + ": " + parsed.message
      : parsed.message;
  }
  return raw;
}

function providerChoices(value: unknown): ConversationProviderChoice[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((item) => {
    if (!item || typeof item !== "object") return [];
    const record = item as Record<string, unknown>;
    if (typeof record.id !== "string" || typeof record.label !== "string") return [];
    return [
      {
        id: record.id,
        label: record.label,
        model: typeof record.model === "string" ? record.model : "",
      },
    ];
  });
}

/** Read the actionable provider ownership state attached by the desktop backend. */
export function conversationRecovery(error: unknown): ConversationRecovery | null {
  const parsed = parseDesktopError(error);
  const details =
    parsed?.details && typeof parsed.details === "object"
      ? (parsed.details as Record<string, unknown>)
      : null;
  if (!details || typeof parsed?.code !== "string") return null;

  const threadId = typeof details.threadId === "string" ? details.threadId : "";
  const providers = providerChoices(details.availableProviders);
  if (parsed.code === "thread_provider_unknown" && threadId) {
    return { kind: "unknown_owner", threadId, providers };
  }

  const providerId = typeof details.providerId === "string" ? details.providerId : "";
  const providerLabel =
    typeof details.providerLabel === "string" ? details.providerLabel : providerId;
  if (parsed.code === "provider_unavailable" && threadId && providerId) {
    return {
      kind: "owner_unavailable",
      threadId,
      providerId,
      providerLabel,
      configured: details.configured === true,
      providers,
    };
  }
  return null;
}

export function shouldOfferProviderReconnect(error: unknown): boolean {
  const message = rawInvokeError(error).toLowerCase();
  return (
    message.includes("needs connect again") ||
    message.includes("needs to be reconnected") ||
    message.includes("auth_unavailable")
  );
}
