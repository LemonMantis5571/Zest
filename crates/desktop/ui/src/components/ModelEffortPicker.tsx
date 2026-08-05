import { useEffect, useId, useRef, useState } from "react";
import { CheckIcon, ChevronDownIcon, RotateCcwIcon } from "lucide-react";

import {
  DEFAULT_EFFORT,
  chipLabel,
  effortsForModel,
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
  const labelId = useId();
  const modelOptions = modelOptionsFromCapabilities(models);
  const effortOptions = effortsForModel(models, model);
  const supportsEffort = effortOptions.length > 0;
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
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? labelId : undefined}
        title={supportsEffort ? "Model and reasoning effort" : "Model"}
        className={cn(
          "inline-flex max-w-[260px] cursor-pointer items-center gap-1 rounded-md px-2 py-1 text-xs text-muted-foreground outline-none transition-colors",
          "hover:bg-secondary hover:text-foreground",
          "focus-visible:ring-2 focus-visible:ring-ring/50",
          open && "bg-secondary text-foreground",
          "disabled:pointer-events-none disabled:cursor-not-allowed disabled:opacity-50"
        )}
        onClick={() => setOpen((value) => !value)}
      >
        <span className="truncate">{supportsEffort ? chipLabel(model, effort) : modelLabel(model)}</span>
        <ChevronDownIcon className="size-2.5 shrink-0 opacity-70" />
      </button>

      {open ? (
        <div
          id={labelId}
          role="dialog"
          aria-label="Model and effort"
          className="absolute bottom-[calc(100%+8px)] left-0 z-50 w-[220px] rounded-lg border border-border bg-popover p-1 text-popover-foreground shadow-lg"
        >
          <div className="px-1.5 py-1 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
            Model
          </div>
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
                    "flex w-full items-center gap-2 rounded-md px-1.5 py-1.5 text-left text-sm outline-none",
                    "hover:bg-accent hover:text-accent-foreground",
                    selected && "bg-accent/70"
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

              <div className="px-1.5 py-1 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
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
                        "flex w-full items-center gap-2 rounded-md px-1.5 py-1.5 text-left text-sm outline-none",
                        "hover:bg-accent hover:text-accent-foreground",
                        selected && "bg-accent/70"
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
            className="flex w-full items-center gap-2 rounded-md px-1.5 py-1.5 text-left text-sm outline-none hover:bg-accent hover:text-accent-foreground"
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
