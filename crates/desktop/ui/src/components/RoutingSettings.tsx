import {
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
  type RefObject,
} from "react";
import { createPortal } from "react-dom";
import {
  CheckIcon,
  ChevronDownIcon,
  PlusIcon,
  WandSparklesIcon,
  XIcon,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import { getBackend } from "@/lib/backend";
import type { RoutingRule, RoutingView } from "@/lib/types";
import { cn } from "@/lib/utils";

const backend = getBackend();

/** Task kinds offered in the Settings picker (select-only). */
const KIND_OPTIONS = ["planning", "frontend", "implementation", "review", "mechanical"];

function ruleKey(rule: RoutingRule, index: number) {
  return `${index}:${rule.kind}:${rule.provider}`;
}

/**
 * Editor for `[routing]` in the user config.
 *
 * Rust owns validation — an unroutable rule is rejected on save rather than
 * accepted and left to fail mid-delegation. Pickers are fed by each provider's
 * real catalogue so the common mistakes cannot be expressed here at all.
 *
 * Panels use fixed positioning (not absolute-in-scroll) so they are not clipped
 * by the Settings drawer. Portal-based menus are avoided for webview stability.
 */
type Props = {
  /** Provider this chat is pinned to — used for same-account warnings. */
  sessionProvider: string;
  /** Rebuilds the session so a saved change takes effect without a restart. */
  onApply?: () => Promise<void>;
};

export function RoutingSettings({ sessionProvider, onApply }: Props) {
  const [view, setView] = useState<RoutingView | null>(null);
  const [rules, setRules] = useState<RoutingRule[]>([]);
  const [delegation, setDelegation] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [applying, setApplying] = useState(false);
  const [applied, setApplied] = useState(false);
  /** Only one menu open across the editor — keeps the narrow drawer readable. */
  const [openMenu, setOpenMenu] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void backend
      .routingConfig()
      .then((next) => {
        if (cancelled) return;
        setView(next);
        setRules(next.rules);
        setDelegation(next.delegation);
      })
      .catch(() => {
        if (!cancelled) setError("Could not load routing settings. Try again.");
      });
    return () => {
      cancelled = true;
    };
  }, []);

  function mutate(next: RoutingRule[]) {
    setRules(next);
    setSaved(false);
    setApplied(false);
    setError(null);
    setOpenMenu(null);
  }

  function updateRule(index: number, patch: Partial<RoutingRule>) {
    mutate(rules.map((r, i) => (i === index ? { ...r, ...patch } : r)));
  }

  async function save() {
    setSaving(true);
    setError(null);
    setOpenMenu(null);
    try {
      const next = await backend.setRoutingConfig(delegation, rules);
      setView(next);
      setRules(next.rules);
      setDelegation(next.delegation);
      setSaved(true);
      setApplied(false);
    } catch {
      setError("Could not save routing settings. Try again.");
    } finally {
      setSaving(false);
    }
  }

  if (!view) {
    return (
      <div className="text-[11px] text-muted-foreground">
        {error ?? "Loading routing…"}
      </div>
    );
  }

  const providers = view.providers;
  const canDelegate = providers.length > 1;
  const modelsFor = (id: string) =>
    providers.find((p) => p.id === id)?.models ?? [];
  const effortsFor = (id: string) =>
    providers.find((p) => p.id === id)?.efforts ?? [];
  /** What the rule really uses — an empty model means the provider's default. */
  const resolvedModel = (rule: RoutingRule) =>
    rule.model ||
    providers.find((p) => p.id === rule.provider)?.defaultModel ||
    "";

  return (
    <div className="space-y-3">
      <label
        className={cn(
          "flex cursor-pointer items-start gap-2.5 rounded-lg border border-border/80 bg-card/80 px-3 py-2.5",
          !canDelegate && "cursor-not-allowed opacity-60"
        )}
      >
        <input
          type="checkbox"
          className="mt-0.5"
          checked={delegation}
          disabled={!canDelegate}
          onChange={(e) => {
            setDelegation(e.target.checked);
            setSaved(false);
            setApplied(false);
            setError(null);
          }}
        />
        <span className="min-w-0 flex-1">
          <span className="block text-sm font-medium">Allow delegation</span>
          <span className="mt-0.5 block text-[11px] leading-snug text-muted-foreground">
            {canDelegate
              ? "Allows tasks to be sent to another provider. Turn this off to keep all work with the current provider."
              : "Add another provider to enable task routing."}
          </span>
        </span>
      </label>

      {view.projectScoped ? (
        <div className="rounded-lg border border-[var(--warning,#c78a2a)]/40 bg-[var(--warning,#c78a2a)]/10 px-3 py-2 text-[11px] leading-snug text-muted-foreground">
          This project uses its own <code>zest.toml</code>. Changes here will not
          affect this project.
        </div>
      ) : null}

      {/* Two different things sound like "the default" and people conflate
          them — the provider this chat is pinned to (picked in the launcher)
          and the fallback a delegated task uses when no rule matches. State
          both, so a warning about one is never read as a claim about the other. */}
      {canDelegate ? (
        <div className="flex flex-wrap items-center gap-x-4 gap-y-1 rounded-lg border border-border/60 px-3 py-2 text-[11px] text-muted-foreground">
          <span>
            This conversation uses{" "}
            <code className="text-foreground">{sessionProvider || "—"}</code>
          </span>
          <span>
            Unmatched tasks use{" "}
            <code className="text-foreground">
              {view.defaultProvider || "—"}
            </code>
          </span>
        </div>
      ) : null}

      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <span className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
            Rules
          </span>
          <span className="text-[10px] text-muted-foreground/70">
            First matching rule is used
          </span>
        </div>

        {rules.length === 0 ? (
          <p className="text-[11px] leading-snug text-muted-foreground">
            No rules yet. Routed tasks use the default provider.
          </p>
        ) : null}

        {rules.map((rule, index) => {
          const models = modelsFor(rule.provider);
          const efforts = effortsFor(rule.provider);
          const id = `rule-${index}`;
          return (
            <div
              key={ruleKey(rule, index)}
              className="rounded-lg border border-border/80 bg-card/80 px-2.5 py-2 animate-in fade-in duration-150"
            >
              <div className="mb-2 flex items-center justify-between gap-2">
                <span className="min-w-0 flex-1 truncate text-[10px] font-medium uppercase tracking-wide text-muted-foreground/70">
                  {/* Resolve what the rule actually does. "Provider default"
                      hides which model that is, which is most of the value. */}
                  {rule.kind.trim() || "new rule"} → {rule.provider || "…"}
                  <span className="ml-1 font-mono normal-case text-muted-foreground/60">
                    {resolvedModel(rule) ? `· ${resolvedModel(rule)}` : ""}
                    {rule.effort ? ` · ${rule.effort}` : ""}
                  </span>
                </span>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-sm"
                  title="Remove rule"
                  className="shrink-0"
                  onClick={() => mutate(rules.filter((_, i) => i !== index))}
                >
                  <XIcon className="size-3.5" />
                </Button>
              </div>

              {/* Not a warning. Delegating to the provider you are already on
                  is legitimate — the worker starts with no conversation
                  history, which is context isolation rather than a different
                  model. Only the expectation can be wrong, so state the fact
                  and let the reader decide. */}
              {rule.provider &&
              sessionProvider &&
              rule.provider === sessionProvider ? (
                <p className="mb-2 text-[10px] leading-snug text-muted-foreground/80">
                  This uses the same provider as the current conversation, but it
                  starts without the conversation history.
                </p>
              ) : null}

              <div className="space-y-1.5">
                <FieldRow label="Kind">
                  <MiniSelect
                    menuId={`${id}-kind`}
                    openMenu={openMenu}
                    setOpenMenu={setOpenMenu}
                    label="Kind"
                    value={rule.kind}
                    display={rule.kind || "Choose kind"}
                    options={[
                      // Keep a legacy/custom kind visible until the user picks a listed one.
                      ...(rule.kind && !KIND_OPTIONS.includes(rule.kind)
                        ? [{ value: rule.kind, label: rule.kind }]
                        : []),
                      ...KIND_OPTIONS.map((kind) => ({ value: kind, label: kind })),
                    ]}
                    onChange={(kind) => updateRule(index, { kind })}
                    mono
                  />
                </FieldRow>
                <FieldRow label="Provider">
                  <MiniSelect
                    menuId={`${id}-provider`}
                    openMenu={openMenu}
                    setOpenMenu={setOpenMenu}
                    label="Provider"
                    value={rule.provider}
                    display={rule.provider || "Choose provider"}
                    options={providers.map((p) => ({ value: p.id, label: p.id }))}
                    onChange={(provider) =>
                      updateRule(index, { provider, model: "", effort: "" })
                    }
                  />
                </FieldRow>
                <FieldRow label="Model">
                  <MiniSelect
                    menuId={`${id}-model`}
                    openMenu={openMenu}
                    setOpenMenu={setOpenMenu}
                    label="Model"
                    value={rule.model}
                    display={rule.model || "Provider default"}
                    options={[
                      { value: "", label: "Provider default" },
                      ...models.map((m) => ({ value: m, label: m })),
                    ]}
                    onChange={(model) => updateRule(index, { model })}
                    mono
                  />
                </FieldRow>
                <FieldRow label="Effort">
                  <MiniSelect
                    menuId={`${id}-effort`}
                    openMenu={openMenu}
                    setOpenMenu={setOpenMenu}
                    label="Effort"
                    value={rule.effort}
                    display={rule.effort || "Provider default"}
                    options={[
                      { value: "", label: "Provider default" },
                      ...efforts.map((eff) => ({ value: eff, label: eff })),
                    ]}
                    onChange={(effort) => updateRule(index, { effort })}
                  />
                </FieldRow>
              </div>
            </div>
          );
        })}

        <Button
          type="button"
          variant="ghost"
          size="sm"
          disabled={providers.length === 0}
          onClick={() =>
            mutate([
              ...rules,
              {
                kind: KIND_OPTIONS[0] ?? "planning",
                provider: providers[0]?.id ?? "",
                model: "",
                effort: "",
                prompt: "",
              },
            ])
          }
        >
          <PlusIcon className="size-3.5" /> Add rule
        </Button>

        {rules.length === 0 && canDelegate ? (
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={() => {
              void backend
                .suggestedRouting()
                .then((suggested) => {
                  if (suggested.length > 0) mutate(suggested);
                })
                .catch(() => setError("Could not suggest routing rules. Try again."));
            }}
          >
            <WandSparklesIcon className="size-3.5" /> Suggest rules
          </Button>
        ) : null}
      </div>

      {error ? (
        <p className="text-[11px] leading-snug text-destructive">{error}</p>
      ) : null}

      <div className="flex items-center justify-between gap-2">
        <span className="min-w-0 truncate text-[10px] text-muted-foreground/70">
          {view.configPath}
        </span>
        <Button type="button" size="sm" disabled={saving} onClick={() => void save()}>
          {saving ? "Saving…" : "Save routing"}
        </Button>
      </div>

      {/* A save that silently does nothing until restart is a trap. The tool
          registry is built once per session, so applying means rebuilding it —
          the open chat is reloaded from disk and survives. */}
      {saved ? (
        <div className="flex items-center justify-between gap-2 rounded-lg border border-border/80 bg-card/80 px-3 py-2">
          <span className="min-w-0 flex-1 text-[11px] leading-snug text-muted-foreground">
            {applied
              ? "Applied. New routed tasks use these rules."
              : "Saved. Apply now to use these rules in this conversation."}
          </span>
          {!applied && onApply ? (
            <Button
              type="button"
              size="sm"
              disabled={applying}
              onClick={() => {
                setApplying(true);
                setError(null);
                void onApply()
                  .then(() => setApplied(true))
                  .catch(() => setError("Could not apply routing settings. Try again."))
                  .finally(() => setApplying(false));
              }}
            >
              {applying ? "Applying…" : "Apply now"}
            </Button>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

function FieldRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid grid-cols-[4.25rem_minmax(0,1fr)] items-center gap-2">
      <span className="text-[10px] font-medium uppercase tracking-wide text-muted-foreground/70">
        {label}
      </span>
      <div className="min-w-0">{children}</div>
    </div>
  );
}

type MenuControl = {
  menuId: string;
  openMenu: string | null;
  setOpenMenu: (id: string | null) => void;
};

function useAnchoredMenu({ menuId, openMenu, setOpenMenu }: MenuControl) {
  const open = openMenu === menuId;
  const rootRef = useRef<HTMLDivElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const [style, setStyle] = useState<CSSProperties | null>(null);

  useLayoutEffect(() => {
    if (!open) {
      setStyle(null);
      return;
    }
    const el = rootRef.current;
    if (!el) return;

    const place = () => {
      const rect = el.getBoundingClientRect();
      const width = Math.max(rect.width, 168);
      const maxHeight = 220;
      const gap = 6;
      const spaceBelow = window.innerHeight - rect.bottom - gap;
      const spaceAbove = rect.top - gap;
      const openUp = spaceBelow < 160 && spaceAbove > spaceBelow;
      const left = Math.max(8, Math.min(rect.left, window.innerWidth - width - 8));
      // Explicit top/bottom: React does not clear omitted style keys.
      // Portal to body: Settings uses transform animation, which breaks
      // position:fixed when the menu stays inside the drawer.
      setStyle({
        position: "fixed",
        top: openUp ? "auto" : rect.bottom + gap,
        bottom: openUp ? window.innerHeight - rect.top + gap : "auto",
        left,
        width,
        maxHeight,
        zIndex: 200,
      });
    };

    place();
    window.addEventListener("resize", place);
    document.addEventListener("scroll", place, true);
    return () => {
      window.removeEventListener("resize", place);
      document.removeEventListener("scroll", place, true);
    };
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target;
      if (!(target instanceof Node)) return;
      if (rootRef.current?.contains(target)) return;
      if (panelRef.current?.contains(target)) return;
      setOpenMenu(null);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpenMenu(null);
    };
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [open, setOpenMenu]);

  function toggle() {
    setOpenMenu(open ? null : menuId);
  }

  return {
    open,
    rootRef,
    panelRef,
    style,
    toggle,
    close: () => setOpenMenu(null),
  };
}

function MenuPanel({
  id,
  label,
  style,
  panelRef,
  children,
}: {
  id: string;
  label: string;
  style: CSSProperties;
  panelRef: RefObject<HTMLDivElement | null>;
  children: ReactNode;
}) {
  return createPortal(
    <div
      ref={panelRef}
      id={id}
      role="listbox"
      aria-label={label}
      style={style}
      className={cn(
        "overflow-y-auto rounded-lg border border-border/80 bg-popover p-1 text-popover-foreground shadow-xl",
        "animate-in fade-in zoom-in-95 duration-150"
      )}
    >
      {children}
    </div>,
    document.body
  );
}

function MiniSelect({
  menuId,
  openMenu,
  setOpenMenu,
  label,
  value,
  display,
  options,
  onChange,
  mono,
}: MenuControl & {
  label: string;
  value: string;
  display: string;
  options: { value: string; label: string }[];
  onChange: (value: string) => void;
  mono?: boolean;
}) {
  const { open, rootRef, panelRef, style, toggle, close } = useAnchoredMenu({
    menuId,
    openMenu,
    setOpenMenu,
  });
  const listId = useId();

  return (
    <div ref={rootRef} className="relative min-w-0">
      <button
        type="button"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? listId : undefined}
        title={label}
        onClick={toggle}
        className={cn(
          "flex w-full cursor-pointer items-center gap-1 rounded-md border border-border/60 bg-background/40 px-2 py-1.5 text-left text-[12px] outline-none transition-colors",
          "hover:bg-accent/50 focus-visible:ring-2 focus-visible:ring-ring/40",
          open && "bg-accent/60 ring-2 ring-ring/40",
          mono && "font-mono text-[11px]"
        )}
      >
        <span className="min-w-0 flex-1 truncate">{display}</span>
        <ChevronDownIcon
          className={cn(
            "size-3 shrink-0 text-muted-foreground transition-transform duration-150",
            open && "rotate-180"
          )}
        />
      </button>
      {open && style ? (
        <MenuPanel id={listId} label={label} style={style} panelRef={panelRef}>
          {options.map((option) => {
            const selected = option.value === value;
            return (
              <button
                key={option.value || "__empty"}
                type="button"
                role="option"
                aria-selected={selected}
                onClick={() => {
                  onChange(option.value);
                  close();
                }}
                className={cn(
                  "flex w-full cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 text-left text-[12px] outline-none transition-colors",
                  "hover:bg-accent/70 focus-visible:bg-accent/70",
                  selected && "bg-accent/50",
                  mono && "font-mono text-[11px]"
                )}
              >
                <span className="min-w-0 flex-1 truncate">{option.label}</span>
                {selected ? <CheckIcon className="size-3 shrink-0 text-primary" /> : null}
              </button>
            );
          })}
        </MenuPanel>
      ) : null}
    </div>
  );
}
