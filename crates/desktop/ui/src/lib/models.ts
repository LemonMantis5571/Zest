export type EffortId = "low" | "medium" | "high" | "xhigh" | "max";

export type ModelOption = {
  id: string;
  label: string;
  shortLabel: string;
};

export type EffortOption = {
  id: EffortId;
  label: string;
  shortLabel: string;
};

/** Codex models available through the CLIProxyAPI Messages path. */
export const CODEX_MODELS: ModelOption[] = [
  { id: "gpt-5.6-sol", label: "5.6 Sol", shortLabel: "Sol" },
  { id: "gpt-5.6-terra", label: "5.6 Terra", shortLabel: "Terra" },
  { id: "gpt-5.6-luna", label: "5.6 Luna", shortLabel: "Luna" },
  { id: "gpt-5.5", label: "5.5", shortLabel: "5.5" },
  { id: "gpt-5.4", label: "5.4", shortLabel: "5.4" },
  { id: "gpt-5.4-mini", label: "5.4 Mini", shortLabel: "Mini" },
];

export const EFFORTS: EffortOption[] = [
  { id: "low", label: "Low", shortLabel: "Low" },
  { id: "medium", label: "Medium", shortLabel: "Med" },
  { id: "high", label: "High", shortLabel: "High" },
  { id: "xhigh", label: "Extra high", shortLabel: "XHigh" },
  { id: "max", label: "Max", shortLabel: "Max" },
];

export const DEFAULT_CODEX_MODEL = "gpt-5.6-sol";
export const DEFAULT_EFFORT: EffortId = "high";

export function modelLabel(modelId: string): string {
  return CODEX_MODELS.find((m) => m.id === modelId)?.label ?? modelId;
}

export function effortLabel(effortId: string): string {
  return EFFORTS.find((e) => e.id === effortId)?.label ?? effortId;
}

export function effortShort(effortId: string): string {
  return EFFORTS.find((e) => e.id === effortId)?.shortLabel ?? effortId;
}

export function chipLabel(modelId: string, effortId: string): string {
  return `${modelLabel(modelId)} · ${effortShort(effortId)}`;
}

export function isEffortId(value: string): value is EffortId {
  return EFFORTS.some((e) => e.id === value);
}

export function providerSupportsModelPicker(provider: string): boolean {
  return provider === "codex" || provider === "fixture";
}
