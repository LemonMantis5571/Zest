import { useEffect, useId, useRef, useState } from "react";
import { CheckIcon, ChevronDownIcon, RotateCcwIcon } from "lucide-react";

import {
  DEFAULT_EFFORT,
  capabilityForModel,
  chipLabel,
  effortsForModel,
  formatContextWindow,
  modelLabel,
  modelOptionsFromCapabilities,
  type EffortId,
  type ModelCapability,
} from "@/lib/models";
import { cn } from "@/lib/utils";

type Props = {
  model: string;
  effort: EffortId;
  models?: ModelCapability[];
  defaultModel?: string;
  disabled?: boolean;
  onModelChange: (model: string) => void;
  onEffortChange: (effort: EffortId) => void;
  onReset?: () => void;
};

/**
 * Plain positioned panel. Portal-based menus have been crashing the desktop
 * webview on open.
 * Model/effort availability comes from Rust; labels are display-only.
 */
export function ModelEffortPicker({
  model,
  effort,
  models,
  defaultModel,
  disabled,
  onModelChange,
  onEffortChange,
  onReset,
}: Props) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const panelId = useId();
  const modelOptions = modelOptionsFromCapabilities(models);
  const effortOptions = effortsForModel(models, model);
  const capability = capabilityForModel(models, model);
  const contextLabel = formatContextWindow(capability?.contextWindow);
  const supportsEffort = effortOptions.length > 0;
  const pickerLabel = supportsEffort ? "Model and effort" : "Model";
  const resetModel = defaultModel ?? modelOptions[0]?.id ?? model;

  useEffect(() => {
    if (!open) return;

    const onPointerDown = (event: PointerEvent) => {
      const root = rootRef.current;
      if (!root) return;
      if (event.target instanceof Node && !root.contains(event.target)) {
        setOpen(false);
      }
    };

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };

    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);

  function applyModel(next: string) {
    setOpen(false);
    if (next !== model) onModelChange(next);
  }

  function applyEffort(next: EffortId) {
    setOpen(false);
    if (next !== effort) onEffortChange(next);
  }

  function reset() {
    setOpen(false);
    onModelChange(resetModel);
    onEffortChange(DEFAULT_EFFORT);
    onReset?.();
  }

  return (
    <div ref={rootRef} className="relative">
      <button
        type="button"
        disabled={disabled}
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-controls={open ? panelId : undefined}
        title={pickerLabel}
        className={cn(
          "inline-flex min-h-8 max-w-[260px] cursor-pointer items-center gap-2 rounded-md px-2.5 py-1.5 text-xs font-medium text-foreground/85 outline-none transition-colors",
          "hover:bg-secondary/60 hover:text-foreground",
          "focus-visible:ring-2 focus-visible:ring-ring/50 focus-visible:ring-offset-1 focus-visible:ring-offset-background",
          open && "bg-secondary/70 text-foreground",
          "disabled:pointer-events-none disabled:cursor-not-allowed disabled:opacity-50"
        )}
        onClick={() => setOpen((value) => !value)}
      >
        <span className="truncate">{supportsEffort ? chipLabel(model, effort) : modelLabel(model)}</span>
        <ChevronDownIcon className="size-3 shrink-0 opacity-60" />
      </button>

      {open ? (
        <div
          id={panelId}
          role="dialog"
          aria-label={pickerLabel}
          className="absolute bottom-[calc(100%+8px)] left-0 z-50 w-[240px] overflow-hidden rounded-xl border border-border/90 bg-popover p-1.5 text-popover-foreground shadow-xl"
        >
          <div className="px-2 py-1.5 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
            Model
          </div>
          {capability ? (
            <div className="mx-1 mb-1.5 rounded-lg border border-border/50 bg-secondary/60 px-2.5 py-2 text-[10px] leading-relaxed text-muted-foreground">
              <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5">
                {contextLabel ? <span>{contextLabel}</span> : null}
                <span>{capability.supportsTools ? "Tools" : "Text only"}</span>
                {capability.supportsVision ? <span>Vision</span> : null}
              </div>
              {!supportsEffort ? (
                <div className="mt-0.5 text-[10px] text-muted-foreground/75">
                  This model manages reasoning automatically
                </div>
              ) : null}
            </div>
          ) : null}
          <div role="listbox" aria-label="Model" className="flex flex-col">
            {modelOptions.map((item) => {
              const selected = item.id === model;
              return (
                <button
                  key={item.id}
                  type="button"
                  role="option"
                  aria-selected={selected}
                  className={cn(
                    "flex w-full items-center gap-2 rounded-lg px-2 py-2 text-left text-sm outline-none transition-colors",
                    "hover:bg-accent hover:text-accent-foreground focus-visible:bg-accent focus-visible:text-accent-foreground",
                    selected && "bg-accent/80"
                  )}
                  onClick={() => applyModel(item.id)}
                >
                  <span className="flex-1 truncate">{item.label}</span>
                  {selected ? <CheckIcon className="size-3.5 shrink-0" /> : null}
                </button>
              );
            })}
          </div>

          {supportsEffort ? (
            <>
              <div className="my-1 h-px bg-border" />

              <div className="px-2 py-1.5 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
                Effort
              </div>
              <div role="listbox" aria-label="Effort" className="flex flex-col">
                {effortOptions.map((item) => {
                  const selected = item.id === effort;
                  return (
                    <button
                      key={item.id}
                      type="button"
                      role="option"
                      aria-selected={selected}
                      className={cn(
                        "flex w-full items-center gap-2 rounded-lg px-2 py-2 text-left text-sm outline-none transition-colors",
                        "hover:bg-accent hover:text-accent-foreground focus-visible:bg-accent focus-visible:text-accent-foreground",
                        selected && "bg-accent/80"
                      )}
                      onClick={() => applyEffort(item.id)}
                    >
                      <span className="flex-1 truncate">{item.label}</span>
                      {selected ? <CheckIcon className="size-3.5 shrink-0" /> : null}
                    </button>
                  );
                })}
              </div>
            </>
          ) : null}

          <div className="my-1 h-px bg-border" />

          <button
            type="button"
            className="flex w-full items-center gap-2 rounded-lg px-2 py-2 text-left text-sm outline-none transition-colors hover:bg-accent hover:text-accent-foreground focus-visible:bg-accent focus-visible:text-accent-foreground"
            onClick={reset}
          >
            <RotateCcwIcon className="size-3.5" />
            Reset to default
          </button>
        </div>
      ) : null}
    </div>
  );
}
