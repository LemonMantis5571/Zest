import { useState } from "react";
import { KeyRoundIcon } from "lucide-react";

import { Button } from "@/components/ui/button";
import { getBackend } from "@/lib/backend";

type Preset = "deepseek" | "openai" | "custom";

const PRESETS: Record<Preset, { label: string; id: string; baseUrl: string; model: string; models: string[] }> = {
  deepseek: {
    label: "DeepSeek",
    id: "deepseek",
    baseUrl: "https://api.deepseek.com",
    model: "deepseek-chat",
    models: ["deepseek-chat", "deepseek-reasoner"],
  },
  openai: {
    label: "OpenAI",
    id: "openai",
    baseUrl: "https://api.openai.com/v1",
    model: "gpt-5",
    models: [],
  },
  custom: { label: "Custom", id: "custom", baseUrl: "", model: "", models: [] },
};

type Props = { onDone: (id: string) => Promise<void>; onCancel: () => void };

export function ApiProviderForm({ onDone, onCancel }: Props) {
  const [preset, setPreset] = useState<Preset>("deepseek");
  const [id, setId] = useState(PRESETS.deepseek.id);
  const [baseUrl, setBaseUrl] = useState(PRESETS.deepseek.baseUrl);
  const [model, setModel] = useState(PRESETS.deepseek.model);
  const [models, setModels] = useState(PRESETS.deepseek.models.join(", "));
  const [credential, setCredential] = useState(PRESETS.deepseek.id);
  const [key, setKey] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const selectPreset = (next: Preset) => {
    const value = PRESETS[next];
    setPreset(next);
    setId(value.id);
    setBaseUrl(value.baseUrl);
    setModel(value.model);
    setModels(value.models.join(", "));
    setCredential(value.id);
  };

  return (
    <form
      className="border-t border-border/60 p-3"
      onSubmit={(event) => {
        event.preventDefault();
        setSaving(true);
        setError(null);
        void getBackend()
          .configureApiProvider({
            id,
            baseUrl,
            model,
            models: models.split(",").map((value) => value.trim()).filter(Boolean),
            credential,
            key,
          })
          .then(async () => {
            setKey("");
            await onDone(id.trim());
          })
          .catch((err) => setError(String(err)))
          .finally(() => setSaving(false));
      }}
    >
      <div className="mb-2 flex items-center gap-2 text-sm font-semibold">
        <KeyRoundIcon className="size-4 text-primary" />
        Add API provider
      </div>
      <div className="mb-3 flex gap-1 rounded-md bg-secondary/50 p-1">
        {(Object.keys(PRESETS) as Preset[]).map((item) => (
          <button
            key={item}
            type="button"
            className={`flex-1 rounded px-2 py-1.5 text-[11px] font-medium ${preset === item ? "bg-background text-foreground shadow-sm" : "text-muted-foreground"}`}
            onClick={() => selectPreset(item)}
          >
            {PRESETS[item].label}
          </button>
        ))}
      </div>
      <div className="grid gap-2">
        {preset === "custom" ? (
          <input value={id} onChange={(e) => setId(e.target.value)} placeholder="Provider id (e.g. local)" className="field" />
        ) : null}
        <input value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)} placeholder="https://api.example.com/v1" className="field" type="url" />
        <input value={model} onChange={(e) => setModel(e.target.value)} placeholder="Default model" className="field" />
        <input value={models} onChange={(e) => setModels(e.target.value)} placeholder="Allowed models (comma separated, optional)" className="field" />
        <input value={key} onChange={(e) => setKey(e.target.value)} placeholder="API key" className="field" type="password" autoComplete="off" />
      </div>
      <p className="mt-2 text-[11px] leading-relaxed text-muted-foreground">
        The key is stored securely and never written to zest.toml.
      </p>
      {error ? <p className="mt-2 text-xs text-destructive">{error}</p> : null}
      <div className="mt-3 flex justify-end gap-2">
        <Button type="button" size="sm" variant="ghost" disabled={saving} onClick={onCancel}>Cancel</Button>
        <Button type="submit" size="sm" disabled={saving || !key.trim() || !id.trim() || !baseUrl.trim() || !model.trim()}>
          {saving ? "Saving…" : "Save provider"}
        </Button>
      </div>
      <style>{`.field { width: 100%; border: 1px solid hsl(var(--border) / .8); border-radius: .375rem; background: hsl(var(--background)); padding: .42rem .6rem; font-size: .75rem; outline: none; } .field:focus { box-shadow: 0 0 0 2px hsl(var(--ring) / .5); }`}</style>
    </form>
  );
}
