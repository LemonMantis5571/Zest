import { useEffect, useRef } from "react";
import { ArrowUpIcon, PlusIcon, SquareIcon } from "lucide-react";

import { ModelEffortPicker } from "@/components/ModelEffortPicker";
import { Button } from "@/components/ui/button";
import { chipLabel, modelLabel, type EffortId } from "@/lib/models";

type Props = {
  value: string;
  model: string;
  effort: EffortId;
  meta: string;
  sending: boolean;
  showModelPicker: boolean;
  /** Disable model/effort while an update is in flight. */
  optionsDisabled?: boolean;
  onChange: (value: string) => void;
  onSubmit: () => void;
  onStop?: () => void;
  onModelChange: (model: string) => void;
  onEffortChange: (effort: EffortId) => void;
};

export function Composer({
  value,
  model,
  effort,
  meta,
  sending,
  showModelPicker,
  optionsDisabled = false,
  onChange,
  onSubmit,
  onStop,
  onModelChange,
  onEffortChange,
}: Props) {
  const ref = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 180)}px`;
  }, [value]);

  const canSend = !sending && value.trim().length > 0;

  return (
    <div className="pointer-events-none absolute inset-x-0 bottom-0 z-10 px-4 pb-3 pt-20">
      <div className="pointer-events-auto mx-auto w-full max-w-[var(--chat-max)]">
        <div className="overflow-visible rounded-2xl border border-border bg-[color-mix(in_srgb,var(--card)_92%,transparent)] shadow-[0_16px_48px_rgba(0,0,0,0.55)] backdrop-blur-xl">
          <textarea
            ref={ref}
            rows={1}
            value={value}
            disabled={sending}
            placeholder="Plan, @ for context, / for commands"
            autoComplete="off"
            className="block max-h-[180px] w-full resize-none bg-transparent px-4 pt-3.5 pb-2 text-sm text-foreground outline-none placeholder:text-muted-foreground disabled:opacity-60"
            onChange={(e) => onChange(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                if (canSend) onSubmit();
              }
            }}
          />
          <div className="flex items-center justify-between gap-2 px-2.5 pb-2.5">
            <div className="relative z-20 flex min-w-0 items-center gap-1 overflow-visible">
              <Button
                type="button"
                variant="ghost"
                size="icon-sm"
                disabled
                title="Context soon"
                className="text-muted-foreground"
              >
                <PlusIcon />
              </Button>
              {showModelPicker ? (
                <ModelEffortPicker
                  model={model}
                  effort={effort}
                  disabled={sending || optionsDisabled}
                  onModelChange={onModelChange}
                  onEffortChange={onEffortChange}
                />
              ) : (
                <span className="truncate px-2 py-1 text-xs text-muted-foreground">
                  {chipLabel(model, effort)}
                </span>
              )}
            </div>
            {sending ? (
              <Button
                type="button"
                size="icon-sm"
                aria-label="Stop"
                title="Stop"
                className="rounded-full"
                onClick={() => onStop?.()}
              >
                <SquareIcon className="size-3.5 fill-current" />
              </Button>
            ) : (
              <Button
                type="button"
                size="icon-sm"
                disabled={!canSend}
                aria-label="Send"
                className="rounded-full"
                onClick={() => {
                  if (canSend) onSubmit();
                }}
              >
                <ArrowUpIcon />
              </Button>
            )}
          </div>
        </div>
        <div className="mt-2 flex items-center justify-between px-1 text-[11px] text-muted-foreground">
          <span>
            {meta}
            {showModelPicker ? ` · ${modelLabel(model)}` : ""}
          </span>
          <span>Enter to send · Shift+Enter for newline</span>
        </div>
      </div>
    </div>
  );
}
