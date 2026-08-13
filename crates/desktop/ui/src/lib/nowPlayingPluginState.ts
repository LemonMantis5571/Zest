import type { PluginView } from "./types";

export type NowPlayingPluginState =
  | "checking"
  | "missing"
  | "unavailable"
  | "disabled"
  | "ready";

export function nowPlayingPluginState(
  checked: boolean,
  plugin: PluginView | null
): NowPlayingPluginState {
  if (!checked) return "checking";
  if (!plugin) return "missing";
  if (!plugin.available) return "unavailable";
  if (!plugin.enabled) return "disabled";
  return "ready";
}
