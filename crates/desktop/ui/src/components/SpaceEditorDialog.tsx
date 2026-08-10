import { useEffect, useId, useState } from "react";

import { Button } from "@/components/ui/button";
import type { SpaceView } from "@/lib/types";

type Props = {
  open: boolean;
  space: SpaceView | null;
  busy?: boolean;
  error?: string | null;
  onSubmit: (name: string, emoji: string) => void;
  onCancel: () => void;
};

/** Portal-free editor so the dialog remains stable in the Tauri WebView. */
export function SpaceEditorDialog({
  open,
  space,
  busy = false,
  error,
  onSubmit,
  onCancel,
}: Props) {
  const titleId = useId();
  const nameId = useId();
  const emojiId = useId();
  const [name, setName] = useState("");
  const [emoji, setEmoji] = useState("");
  const spaceId = space?.id ?? null;
  const spaceName = space?.name ?? "";
  const spaceEmoji = space?.emoji ?? "";

  useEffect(() => {
    if (!open) return;
    setName(spaceName);
    setEmoji(spaceEmoji);
    window.setTimeout(() => document.getElementById(nameId)?.focus(), 0);
  }, [open, spaceId, nameId, spaceName, spaceEmoji]);

  if (!open) return null;

  function submit() {
    if (busy || !name.trim()) return;
    onSubmit(name, emoji);
  }

  return (
    <div className="fixed inset-0 z-[80] flex items-center justify-center p-4">
      <button
        type="button"
        aria-label="Dismiss"
        className="absolute inset-0 cursor-pointer bg-black/55 animate-in fade-in duration-150"
        disabled={busy}
        onClick={() => {
          if (!busy) onCancel();
        }}
      />
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        className="relative w-full max-w-[360px] rounded-xl border border-border bg-[var(--chat-header,#121314)] p-4 shadow-2xl animate-in zoom-in-95 fade-in duration-150"
      >
        <h2 id={titleId} className="text-sm font-semibold tracking-[-0.2px]">
          {space ? "Rename Space" : "Create Space"}
        </h2>
        <div className="mt-4 space-y-3">
          <label className="block text-xs text-muted-foreground" htmlFor={nameId}>
            Name
            <input
              id={nameId}
              value={name}
              maxLength={60}
              disabled={busy}
              onChange={(event) => setName(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                  submit();
                }
                if (event.key === "Escape" && !busy) onCancel();
              }}
              className="mt-1.5 h-9 w-full rounded-md border border-border/80 bg-background/50 px-2.5 text-sm text-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
            />
          </label>
          <label className="block text-xs text-muted-foreground" htmlFor={emojiId}>
            Emoji <span className="text-muted-foreground/70">(optional)</span>
            <input
              id={emojiId}
              value={emoji}
              maxLength={16}
              disabled={busy}
              onChange={(event) => setEmoji(event.target.value)}
              placeholder="🧭"
              className="mt-1.5 h-9 w-full rounded-md border border-border/80 bg-background/50 px-2.5 text-sm text-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
            />
          </label>
          {error ? <p className="text-xs text-destructive">{error}</p> : null}
        </div>
        <div className="mt-5 flex justify-end gap-2">
          <Button type="button" variant="ghost" size="sm" disabled={busy} onClick={onCancel}>
            Cancel
          </Button>
          <Button type="button" size="sm" disabled={busy || !name.trim()} onClick={submit}>
            {busy ? "Saving…" : space ? "Save" : "Create"}
          </Button>
        </div>
      </div>
    </div>
  );
}
