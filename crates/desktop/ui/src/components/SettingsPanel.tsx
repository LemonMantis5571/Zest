import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import {
  BookOpenIcon,
  ChartColumnIcon,
  ChevronRightIcon,
  type LucideIcon,
  ScrollTextIcon,
  ServerIcon,
  SplitIcon,
  UserIcon,
  XIcon,
} from "lucide-react";

import { RoutingSettings } from "@/components/RoutingSettings";
import { Button } from "@/components/ui/button";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { getBackend, type SkillSummary } from "@/lib/backend";
import { chipLabel, type EffortId } from "@/lib/models";
import { optimizeAvatarFile } from "@/lib/optimizeAvatar";
import type {
  ProviderRow,
  SessionInfo,
  UsageSnapshot,
  UserProfile,
} from "@/lib/types";
import { cn } from "@/lib/utils";

type Props = {
  open: boolean;
  session: SessionInfo;
  model: string;
  effort: EffortId;
  sending: boolean;
  profile: UserProfile;
  /** Open the User section first (avatar click). */
  focusUser?: boolean;
  onClose: () => void;
  onChangeProvider: () => void;
  /** Rebuild the session so a routing change applies without a restart. */
  onReloadSession?: () => Promise<void>;
  onReconnect: () => void;
  onOpenFolder: () => void;
  onProfileChange: (profile: UserProfile) => void;
};

const CUSTOM_SOFT_LIMIT = 8000;

function formatAge(secs: number) {
  if (secs < 60) return `${secs}s`;
  if (secs < 3600) return `${Math.floor(secs / 60)}m`;
  return `${Math.floor(secs / 3600)}h`;
}

function SettingsSection({
  title,
  hint,
  icon: Icon,
  defaultOpen = false,
  children,
}: {
  title: string;
  hint?: string;
  icon: LucideIcon;
  defaultOpen?: boolean;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(defaultOpen);

  useEffect(() => {
    if (defaultOpen) setOpen(true);
  }, [defaultOpen]);

  return (
    <Collapsible open={open} onOpenChange={setOpen} className="border-b border-border/50">
      <CollapsibleTrigger
        className={cn(
          "flex w-full cursor-pointer items-center gap-2.5 px-4 py-3 text-left outline-none transition-colors",
          "hover:bg-accent/40 focus-visible:ring-2 focus-visible:ring-ring/40"
        )}
      >
        <ChevronRightIcon
          className={cn(
            "size-3.5 shrink-0 text-muted-foreground transition-transform duration-150",
            open && "rotate-90"
          )}
        />
        <Icon className="size-3.5 shrink-0 text-muted-foreground" aria-hidden />
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
  profile,
  focusUser = false,
  onClose,
  onChangeProvider,
  onReloadSession,
  onReconnect,
  onOpenFolder,
  onProfileChange,
}: Props) {
  const panelRef = useRef<HTMLDivElement>(null);
  const titleId = useId();
  const [provider, setProvider] = useState<ProviderRow | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [customPrompt, setCustomPrompt] = useState("");
  const [savedCustom, setSavedCustom] = useState("");
  const [basePrompt, setBasePrompt] = useState("");
  const [promptPath, setPromptPath] = useState(".zest/system.md");
  const [promptSaving, setPromptSaving] = useState(false);
  const [promptError, setPromptError] = useState<string | null>(null);
  const [promptSavedFlash, setPromptSavedFlash] = useState(false);

  const [skills, setSkills] = useState<SkillSummary[]>([]);
  const [usage, setUsage] = useState<UsageSnapshot | null>(null);
  const [displayName, setDisplayName] = useState(profile.displayName);
  const [avatarDataUrl, setAvatarDataUrl] = useState(profile.avatarDataUrl);
  const [profileSaving, setProfileSaving] = useState(false);
  const [profileError, setProfileError] = useState<string | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!open) return;
    setDisplayName(profile.displayName);
    setAvatarDataUrl(profile.avatarDataUrl);
  }, [open, profile.displayName, profile.avatarDataUrl]);

  useEffect(() => {
    if (!open) return;

    let cancelled = false;
    setLoading(true);
    setError(null);
    setPromptError(null);
    setProfileError(null);

    const backend = getBackend();
    // Settled, not all: these are independent sections, and one of them
    // failing used to blank the other three. The system prompt in particular
    // needs the live session, which is unavailable while a turn streams —
    // that must not take Usage and Skills down with it.
    Promise.allSettled([
      backend.listProviders(),
      backend.getSystemPrompt(),
      backend.listSkills(),
      backend.usageSnapshot(),
    ])
      .then(([rowsR, promptR, skillsR, snapR]) => {
        if (cancelled) return;

        if (rowsR.status === "fulfilled") {
          setProvider(
            rowsR.value.find((p) => p.id === session.provider) ?? null
          );
        } else {
          setError(String(rowsR.reason));
        }

        if (promptR.status === "fulfilled") {
          setCustomPrompt(promptR.value.custom);
          setSavedCustom(promptR.value.custom);
          setBasePrompt(promptR.value.base);
          setPromptPath(promptR.value.customPath);
        } else {
          // Never fake empty settings state — say why it is missing.
          setBasePrompt("");
          setPromptError(String(promptR.reason));
        }

        setSkills(skillsR.status === "fulfilled" ? skillsR.value : []);
        if (skillsR.status === "rejected") setError(String(skillsR.reason));

        setUsage(snapR.status === "fulfilled" ? snapR.value : null);
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
    : "Default Zest rules";

  async function savePrompt() {
    setPromptSaving(true);
    setPromptError(null);
    setPromptSavedFlash(false);
    try {
      const info = await getBackend().setSystemPrompt(customPrompt);
      setCustomPrompt(info.custom);
      setSavedCustom(info.custom);
      setBasePrompt(info.base);
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

  const profileDirty =
    displayName !== profile.displayName || avatarDataUrl !== profile.avatarDataUrl;

  async function saveProfile() {
    setProfileSaving(true);
    setProfileError(null);
    try {
      const next = await getBackend().setUserProfile({
        displayName: displayName.trim(),
        avatarDataUrl,
      });
      onProfileChange(next);
      setDisplayName(next.displayName);
      setAvatarDataUrl(next.avatarDataUrl);
    } catch (err) {
      setProfileError(String(err));
    } finally {
      setProfileSaving(false);
    }
  }

  async function onPickAvatar(file: File | null) {
    if (!file) return;
    setProfileError(null);
    try {
      setAvatarDataUrl(await optimizeAvatarFile(file));
    } catch (err) {
      setProfileError(err instanceof Error ? err.message : String(err));
    }
  }

  return (
    <div className="absolute inset-0 z-40 flex justify-end overflow-hidden">
      <button
        type="button"
        aria-label="Close settings"
        className="absolute inset-0 cursor-pointer bg-black/45 animate-in fade-in duration-150"
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
            title="User"
            icon={UserIcon}
            hint={displayName.trim() || "Name & photo"}
            defaultOpen={focusUser}
          >
            <div className="flex items-center gap-3">
              <button
                type="button"
                className="grid size-14 cursor-pointer place-items-center overflow-hidden rounded-xl bg-card ring-1 ring-border outline-none transition-opacity hover:opacity-90 focus-visible:ring-2 focus-visible:ring-ring/50"
                title="Change avatar"
                onClick={() => fileRef.current?.click()}
              >
                {avatarDataUrl ? (
                  <img src={avatarDataUrl} alt="" className="size-full object-cover" />
                ) : (
                  <span className="text-sm text-muted-foreground">PFP</span>
                )}
              </button>
              <input
                ref={fileRef}
                type="file"
                accept="image/png,image/jpeg,image/webp,image/gif"
                className="hidden"
                onChange={(e) => {
                  void onPickAvatar(e.target.files?.[0] ?? null);
                }}
              />
              <div className="min-w-0 flex-1">
                <label className="mb-1 block text-[11px] text-muted-foreground">
                  Display name
                </label>
                <input
                  value={displayName}
                  onChange={(e) => setDisplayName(e.target.value)}
                  disabled={sending || profileSaving}
                  placeholder="Your name"
                  className="w-full rounded-md border border-border/80 bg-card/80 px-2.5 py-1.5 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
                />
              </div>
            </div>
            {profileError ? (
              <p className="mt-2 text-xs text-destructive">{profileError}</p>
            ) : null}
            <div className="mt-2.5 flex flex-wrap gap-1.5">
              <Button
                type="button"
                size="sm"
                disabled={sending || profileSaving || !profileDirty}
                onClick={() => void saveProfile()}
              >
                {profileSaving ? "Saving…" : "Save profile"}
              </Button>
              {avatarDataUrl ? (
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  disabled={sending || profileSaving}
                  onClick={() => setAvatarDataUrl("")}
                >
                  Remove photo
                </Button>
              ) : null}
            </div>
          </SettingsSection>

          <SettingsSection
            title="Provider"
            icon={ServerIcon}
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
                <Button
                  type="button"
                  size="sm"
                  variant="secondary"
                  disabled={sending}
                  onClick={onOpenFolder}
                >
                  Change folder
                </Button>
                {canConnect ? (
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
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
            title="Routing"
            icon={SplitIcon}
            hint="Send task kinds to different providers"
          >
            <RoutingSettings
              sessionProvider={session.provider}
              onApply={onReloadSession}
            />
          </SettingsSection>

          <SettingsSection
            title="Usage"
            icon={ChartColumnIcon}
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

          <SettingsSection title="System prompt" icon={ScrollTextIcon} hint={promptHint}>
            <p className="mb-2 text-xs leading-relaxed text-muted-foreground">
              Optional project instructions. Saved to{" "}
              <span className="font-mono text-[11px] text-foreground/80">{promptPath}</span>
              . Empty is fine — Zest still uses its built-in coding-agent rules.
              Takes effect on the next message.
            </p>
            {!customPrompt.trim() && basePrompt.trim() ? (
              <div className="mb-2 rounded-lg border border-border/60 bg-card/40 px-3 py-2">
                <div className="mb-1 text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
                  Default (active while empty)
                </div>
                <p className="m-0 whitespace-pre-wrap font-mono text-[11px] leading-relaxed text-muted-foreground">
                  {basePrompt}
                </p>
              </div>
            ) : null}
            <textarea
              value={customPrompt}
              onChange={(e) => setCustomPrompt(e.target.value)}
              disabled={sending || promptSaving}
              rows={7}
              spellCheck={false}
              placeholder={"Optional: You are …\nProject conventions, tone, extra rules…"}
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
            icon={BookOpenIcon}
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

          {error ? (
            <p className="px-4 py-3 text-xs text-destructive">{error}</p>
          ) : null}
          {loading ? (
            <p className="px-4 py-2 text-xs text-muted-foreground">Loading…</p>
          ) : null}
        </div>
      </div>
    </div>
  );
}
