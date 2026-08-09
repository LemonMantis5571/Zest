import { providerMark, type ProviderMark } from "@/lib/providerMarks";
import { cn } from "@/lib/utils";

/**
 * A mark for the provider a chat belongs to.
 *
 * Keyed on the **provider id**, never on the model name. Zest reaches Codex
 * through a gateway and reaches DeepSeek, a local Ollama server, or anything
 * else through the same `openai_compatible` shape — so a model name tells you
 * very little about who is being talked to, while the id is exactly the thing
 * the user configured.
 *
 * These are simple monochrome glyphs drawn here rather than the vendors'
 * official brand SVGs. They identify a connection in a sidebar; they are not
 * anybody's logo, and shipping real trademarked artwork into a desktop bundle
 * invites a licensing question this does not need to raise.
 *
 * Anything unrecognised gets [`GenericMark`]. That is the normal case for a
 * local model, not a failure — a provider Zest has never heard of should look
 * deliberately plain, not broken.
 */

type Props = {
  providerId?: string | null;
  className?: string;
  /**
   * Accessible name. Omit when the icon sits next to the provider's name
   * already, so a screen reader does not read it twice.
   */
  label?: string;
};

/** Two overlapping strokes — the gateway/relay shape Codex is reached through. */
function CodexMark() {
  return (
    <>
      <path d="M4 8.5 8 6l4 2.5v3L8 14l-4-2.5z" />
      <path d="M8 6v8" />
    </>
  );
}

/** A radial burst, echoing Anthropic's spoked mark without reproducing it. */
function ClaudeMark() {
  return (
    <>
      <path d="M8 2.5v11" />
      <path d="M3.2 5.2l9.6 5.6" />
      <path d="M3.2 10.8l9.6-5.6" />
    </>
  );
}

/** A stylised whale tail. */
function DeepSeekMark() {
  return (
    <>
      <path d="M2.5 10.5c3 0 5-1.5 6.5-4" />
      <path d="M9 6.5c1.5 1 3 1.5 4.5 1.5-.5 2.5-2.5 4-5 4" />
    </>
  );
}

/** Four points converging — Gemini's twin-spark idea, redrawn. */
function GeminiMark() {
  return (
    <>
      <path d="M8 2.5c0 3-2.5 5.5-5.5 5.5 3 0 5.5 2.5 5.5 5.5 0-3 2.5-5.5 5.5-5.5-3 0-5.5-2.5-5.5-5.5z" />
    </>
  );
}

/** A plain server/endpoint outline for anything unmapped. */
function GenericMark() {
  return (
    <>
      <rect x="2.75" y="3.25" width="10.5" height="4" rx="1" />
      <rect x="2.75" y="8.75" width="10.5" height="4" rx="1" />
      <path d="M5 5.25h.01M5 10.75h.01" />
    </>
  );
}

/** Which glyph draws each mark. The choosing lives in `providerMarks`. */
const DRAW: Record<ProviderMark, () => React.ReactNode> = {
  codex: CodexMark,
  claude: ClaudeMark,
  deepseek: DeepSeekMark,
  gemini: GeminiMark,
  generic: GenericMark,
};

export function ProviderIcon({ providerId, className, label }: Props) {
  const draw = DRAW[providerMark(providerId)];

  return (
    <svg
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.4}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={cn("size-3.5 shrink-0", className)}
      role={label ? "img" : undefined}
      aria-label={label}
      aria-hidden={label ? undefined : true}
    >
      {draw()}
    </svg>
  );
}

