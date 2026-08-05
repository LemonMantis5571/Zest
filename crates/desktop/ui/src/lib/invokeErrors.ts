export function rawInvokeError(error: unknown): string {
  const raw = String(error);
  try {
    const start = raw.indexOf("{");
    const end = raw.lastIndexOf("}");
    if (start >= 0 && end > start) {
      const parsed = JSON.parse(raw.slice(start, end + 1)) as {
        message?: string;
        code?: string;
      };
      if (parsed.message) {
        return parsed.code ? parsed.code + ": " + parsed.message : parsed.message;
      }
    }
  } catch {
    // Keep the raw value available for internal classification only.
  }
  return raw;
}

export function shouldOfferProviderReconnect(error: unknown): boolean {
  const message = rawInvokeError(error).toLowerCase();
  return (
    message.includes("needs connect again") ||
    message.includes("needs to be reconnected") ||
    message.includes("auth_unavailable")
  );
}
