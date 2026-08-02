import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import { ChevronRightIcon, XIcon } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { getBackend, type SkillSummary } from "@/lib/backend";
import { chipLabel, type EffortId } from "@/lib/models";
import type {
  ProviderRow,
  SessionInfo,
  ThreadSummary,
  UsageSnapshot,
} from "@/lib/types";
import { cn } from "@/lib/utils";

type Props = {
  open: boolean;
  session: SessionInfo;
  model: string;
  effort: EffortId;
  sending: boolean;
  onClose: () => void;
  onNewChat: () => void;
  onChangeProvider: () => void;
  onReconnect: () => void;
  onLoadThread: (id: string) => void;
};

const CUSTOM_SOFT_LIMIT = 8000;

function formatUpdatedAt(epochSecs: number) {
  if (!epochSecs) return "";
  try {
    return new Intl.DateTimeFormat(undefined, {
      month: "short",
      day: "numeric",
      hour: "numeric",
      minute: "2-digit",
    }).format(new Date(epochSecs * 1000));
  } catch {
    return "";
  }
}

function formatAge(secs: number) {
  if (secs < 60) return `${secs}s`;
  if (secs < 3600) return `${Math.floor(secs / 60)}m`;
  return `${Math.floor(secs / 3600)}h`;
}

function threadTitle(thread: ThreadSummary) {
  const title = thread.title?.trim();
  if (title) return title;
  return "Untitled chat";
}

function SettingsSection({
  title,
  hint,
  defaultOpen = false,
  children,
}: {
  title: string;
  hint?: string;
  defaultOpen?: boolean;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(defaultOpen);

  return (
    <Collapsible open={open} onOpenChange={setOpen} className="border-b border-border/50">
      <CollapsibleTrigger
        className={cn(
          "flex w-full items-center gap-2 px-4 py-3 text-left outline-none transition-colors",
          "hover:bg-accent/40 focus-visible:ring-2 focus-visible:ring-ring/40"
        )}
      >
        <ChevronRightIcon
          className={cn(
            "size-3.5 shrink-0 text-muted-foreground transition-transform duration-150",
            open && "rotate-90"
          )}
        />
        <span className="min-w-0 flex-1">
          <span className="block text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
            {title}
          </span>
          {hint && !open ? (
            <span className="mt-0.5 block truncate text-[11px] text-muted-foreground/80">
              {hint}
            </span>
          ) : null}
        </span>
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="px-4 pb-4 pt-3">{children}</div>
      </CollapsibleContent>
    </Collapsible>
  );
}

/**
 * Plain overlay panel — no Base UI Menu/Portal (WebView crash risk).
 */
export function SettingsPanel({
  open,
  session,
  model,
  effort,
  sending,
  onClose,
  onNewChat,
  onChangeProvider,
  onReconnect,
  onLoadThread,
}: Props) {
  const panelRef = useRef<HTMLDivElement>(null);
  const titleId = useId();
  const [provider, setProvider] = useState<ProviderRow | null>(null);
  const [threads, setThreads] = useState<ThreadSummary[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [customPrompt, setCustomPrompt] = useState("");
  const [savedCustom, setSavedCustom] = useState("");
  const [promptPath, setPromptPath] = useState(".zest/system.md");
  const [promptSaving, setPromptSaving] = useState(false);
  const [promptError, setPromptError] = useState<string | null>(null);
  const [promptSavedFlash, setPromptSavedFlash] = useState(false);

  const [skills, setSkills] = useState<SkillSummary[]>([]);
  const [usage, setUsage] = useState<UsageSnapshot | null>(null);

  useEffect(() => {
    if (!open) return;

    let cancelled = false;
    setLoading(true);
    setError(null);
    setPromptError(null);

    const backend = getBackend();
    Promise.all([
      backend.listProviders(),
      backend.listThreads(),
      backend.getSystemPrompt(),
      backend.listSkills(),
      backend.usageSnapshot(),
    ])
      .then(([rows, list, prompt, skillList, snap]) => {
        if (cancelled) return;
        setProvider(rows.find((p) => p.id === session.provider) ?? null);
        setThreads(list);
        setCustomPrompt(prompt.custom);
        setSavedCustom(prompt.custom);
        setPromptPath(prompt.customPath);
        setSkills(skillList);
        setUsage(snap);
      })
      .catch((err) => {
        if (cancelled) return;
        // Surface real load failures — never fake empty settings state.
        setError(String(err));
        setThreads([]);
        setSkills([]);
        setUsage(null);
        setPromptError(String(err));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, [open, session.provider]);

  // Refresh usage after terminal turns without resetting prompt drafts.
  useEffect(() => {
    if (!open || sending) return;
    let cancelled = false;
    getBackend()
      .usageSnapshot()
      .then((snap) => {
        if (!cancelled) setUsage(snap);
      })
      .catch(() => {
        /* keep last good snapshot */
      });
    return () => {
      cancelled = true;
    };
  }, [open, sending]);

  useEffect(() => {
    if (!open) return;

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };

    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [open, onClose]);

  useEffect(() => {
    if (!open) return;
    panelRef.current?.focus();
  }, [open]);

  if (!open) return null;

  const canConnect = provider?.canConnect ?? false;
  const connectLabel =
    provider?.statusKind === "ready" || provider?.statusKind === "unknown"
      ? "Reconnect"
      : "Connect";

  const promptDirty = customPrompt !== savedCustom;
  const overSoftLimit = customPrompt.length > CUSTOM_SOFT_LIMIT;
  const promptHint = savedCustom.trim()
    ? `${savedCustom.trim().slice(0, 42)}${savedCustom.trim().length > 42 ? "…" : ""}`
    : "No custom instructions";

  async function savePrompt() {
    setPromptSaving(true);
    setPromptError(null);
    setPromptSavedFlash(false);
    try {
      const info = await getBackend().setSystemPrompt(customPrompt);
      setCustomPrompt(info.custom);
      setSavedCustom(info.custom);
      setPromptPath(info.customPath);
      setPromptSavedFlash(true);
      window.setTimeout(() => setPromptSavedFlash(false), 1600);
      const nextSkills = await getBackend().listSkills().catch(() => skills);
      setSkills(nextSkills);
    } catch (err) {
      setPromptError(String(err));
    } finally {
      setPromptSaving(false);
    }
  }

  function revertPrompt() {
    setCustomPrompt(savedCustom);
    setPromptError(null);
  }

  return (
    <div className="absolute inset-0 z-40 flex justify-end overflow-hidden">
      <button
        type="button"
        aria-label="Close settings"
        className="absolute inset-0 bg-black/45 animate-in fade-in duration-150"
        onClick={onClose}
      />
      <div
        ref={panelRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        tabIndex={-1}
        className={cn(
          "relative flex h-full w-full max-w-[340px] shrink-0 flex-col border-l border-border bg-[var(--chat-header,#121314)] text-foreground shadow-2xl outline-none",
          "animate-in slide-in-from-right duration-200 ease-out"
        )}
      >
        <header className="flex shrink-0 items-center justify-between border-b border-border/60 px-4 py-3">
          <h2 id={titleId} className="text-sm font-semibold tracking-[-0.2px]">
            Settings
          </h2>
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            title="Close"
            onClick={onClose}
          >
            <XIcon />
          </Button>
        </header>

        <div className="min-h-0 flex-1 overflow-y-auto">
          <SettingsSection
            title="Provider"
            hint={`${session.label} · ${provider?.statusLabel ?? session.provider}`}
          >
            <div className="rounded-lg border border-border/80 bg-card/80 px-3 py-2.5">
              <div className="text-sm font-medium">{session.label}</div>
              <div className="mt-0.5 text-[11px] text-muted-foreground">
                {provider?.statusLabel ?? session.provider}
              </div>
              <div
                className="mt-1 break-all font-mono text-[11px] text-muted-foreground"
                title={session.root}
              >
                {session.root}
              </div>
              <div className="mt-3 flex flex-wrap gap-1.5">
                {canConnect ? (
                  <Button
                    type="button"
                    size="sm"
                    variant="secondary"
                    disabled={sending}
                    onClick={onReconnect}
                  >
                    {connectLabel}
                  </Button>
                ) : null}
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  disabled={sending}
                  onClick={onChangeProvider}
                >
                  Change provider
                </Button>
              </div>
            </div>
            <p className="mt-2 text-xs leading-relaxed text-muted-foreground">
              Using {chipLabel(model, effort)}. Change model and effort in the composer.
            </p>
          </SettingsSection>

          <SettingsSection
            title="Usage"
            hint={
              usage?.providers.length
                ? `${usage.providers.length} account${usage.providers.length === 1 ? "" : "s"}`
                : "Nothing used yet"
            }
          >
            {usage?.providers.length ? (
              <div className="space-y-2">
                {usage.providers.map((row) => (
                  <div
                    key={row.providerId}
                    className="rounded-lg border border-border/80 bg-card/80 px-3 py-2.5"
                  >
                    <div className="text-sm font-medium">{row.providerId}</div>
                    <div className="mt-1.5 space-y-1 text-[11px] text-muted-foreground">
                      <div>
                        <span className="text-foreground/80">Used in Zest</span>
                        {": "}
                        {row.measured.requests}{" "}
                        {row.measured.requests === 1 ? "request" : "requests"} ·{" "}
                        {row.measured.totalTokens.toLocaleString()} tokens
                      </div>
                      <div>
                        {row.headroom.kind === "provider_reported" ? (
                          <>
                            <span className="text-foreground/80">Provider limit</span>
                            {": "}
                            {row.headroom.requestsRemaining != null
                              ? `${row.headroom.requestsRemaining} left`
                              : "shared by your provider"}
                            {row.headroom.ageSecs != null
                              ? ` · updated ${formatAge(row.headroom.ageSecs)} ago`
                              : null}
                          </>
                        ) : (
                          <span className="text-foreground/80">
                            No limit shared by this provider
                          </span>
                        )}
                      </div>
                    </div>
                  </div>
                ))}
                <p className="text-[11px] leading-relaxed text-muted-foreground">
                  This is what Zest used in this app — not your full plan remaining.
                </p>
              </div>
            ) : (
              <p className="text-xs text-muted-foreground">
                {loading ? "Loading…" : "No usage yet. Send a message to start tracking."}
              </p>
            )}
          </SettingsSection>

          <SettingsSection title="System prompt" hint={promptHint}>
            <p className="mb-2 text-xs leading-relaxed text-muted-foreground">
              Custom instructions for this project. Saved to{" "}
              <span className="font-mono text-[11px] text-foreground/80">{promptPath}</span>
              . Takes effect on the next message.
            </p>
            <textarea
              value={customPrompt}
              onChange={(e) => setCustomPrompt(e.target.value)}
              disabled={sending || promptSaving}
              rows={7}
              spellCheck={false}
              placeholder="You are …&#10;Project conventions, tone, extra rules…"
              className={cn(
                "w-full resize-y rounded-lg border border-border/80 bg-card/80 px-3 py-2 font-mono text-[11px] leading-relaxed text-foreground caret-foreground outline-none",
                "placeholder:text-muted-foreground/70 focus-visible:ring-2 focus-visible:ring-ring/50",
                "disabled:opacity-60"
              )}
            />
            <div className="mt-1.5 flex items-center justify-between gap-2 text-[11px] text-muted-foreground">
              <span className={overSoftLimit ? "text-destructive" : undefined}>
                {customPrompt.length.toLocaleString()} chars
                {overSoftLimit ? " · large prompts may hurt cache hit rate" : ""}
              </span>
              {promptSavedFlash ? (
                <span className="text-[var(--success,#27a644)]">Saved — next message uses it</span>
              ) : null}
            </div>
            {promptError ? (
              <p className="mt-1.5 text-xs text-destructive">{promptError}</p>
            ) : null}
            <div className="mt-2.5 flex flex-wrap gap-1.5">
              <Button
                type="button"
                size="sm"
                disabled={sending || promptSaving || !promptDirty}
                onClick={() => void savePrompt()}
              >
                {promptSaving ? "Saving…" : "Save"}
              </Button>
              <Button
                type="button"
                size="sm"
                variant="outline"
                disabled={sending || promptSaving || !promptDirty}
                onClick={revertPrompt}
              >
                Revert
              </Button>
            </div>
          </SettingsSection>

          <SettingsSection
            title="Skills"
            hint={
              skills.length === 0
                ? "None loaded"
                : `${skills.length} skill${skills.length === 1 ? "" : "s"}`
            }
          >
            <p className="mb-2 text-xs leading-relaxed text-muted-foreground">
              Cursor-style <span className="font-mono text-[11px]">SKILL.md</span> folders in{" "}
              <span className="font-mono text-[11px]">.zest/skills/</span> and{" "}
              <span className="font-mono text-[11px]">~/.zest/skills/</span>.
            </p>
            {skills.length === 0 ? (
              <p className="text-xs text-muted-foreground">No skills loaded.</p>
            ) : (
              <ul className="m-0 flex list-none flex-col gap-1.5 p-0">
                {skills.map((skill) => (
                  <li
                    key={`${skill.source}:${skill.name}`}
                    className="rounded-md border border-border/70 bg-card/60 px-2.5 py-2"
                  >
                    <div className="flex items-baseline justify-between gap-2">
                      <span className="truncate text-sm font-medium">{skill.name}</span>
                      <span className="shrink-0 text-[10px] uppercase tracking-wide text-muted-foreground">
                        {skill.source}
                        {skill.inlined ? " · inlined" : ""}
                      </span>
                    </div>
                    <p className="mt-0.5 text-[11px] leading-snug text-muted-foreground">
                      {skill.description}
                    </p>
                  </li>
                ))}
              </ul>
            )}
          </SettingsSection>

          <SettingsSection
            title="Chats"
            hint={`${threads.length} recent`}
          >
            <Button
              type="button"
              size="sm"
              variant="secondary"
              className="mb-3 w-full justify-center"
              disabled={sending}
              onClick={onNewChat}
            >
              New chat
            </Button>
            {loading ? (
              <p className="text-xs text-muted-foreground">Loading…</p>
            ) : error ? (
              <p className="text-xs text-destructive">{error}</p>
            ) : threads.length === 0 ? (
              <p className="text-xs text-muted-foreground">No saved threads yet.</p>
            ) : (
              <ul className="m-0 flex list-none flex-col gap-1 p-0">
                {threads.map((thread) => {
                  const active = thread.id === session.threadId;
                  return (
                    <li key={thread.id}>
                      <button
                        type="button"
                        disabled={sending || active}
                        onClick={() => onLoadThread(thread.id)}
                        className={cn(
                          "flex w-full flex-col gap-0.5 rounded-md px-2.5 py-2 text-left outline-none transition-colors",
                          "hover:bg-accent hover:text-accent-foreground",
                          "focus-visible:ring-2 focus-visible:ring-ring/50",
                          "disabled:pointer-events-none",
                          active && "bg-accent/70 text-foreground"
                        )}
                      >
                        <span className="truncate text-sm">
                          {threadTitle(thread)}
                          {active ? (
                            <span className="ml-1.5 text-[11px] text-muted-foreground">
                              Active
                            </span>
                          ) : null}
                        </span>
                        <span className="text-[11px] text-muted-foreground">
                          {thread.messageCount}{" "}
                          {thread.messageCount === 1 ? "message" : "messages"}
                          {thread.updatedAt
                            ? ` · ${formatUpdatedAt(thread.updatedAt)}`
                            : ""}
                        </span>
                      </button>
                    </li>
                  );
                })}
              </ul>
            )}
          </SettingsSection>
        </div>
      </div>
    </div>
  );
}
