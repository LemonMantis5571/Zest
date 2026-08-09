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

/**
 * OpenAI's knot, reduced to a six-lobed rosette.
 *
 * The real mark is a single interlaced ribbon whose crossings vanish below
 * about 20px. Six lobes on a hexagonal rhythm keeps the silhouette people
 * recognise at the size this actually renders.
 */
function CodexMark() {
  // Three nested subpaths under `evenodd`: hexagonal ring, then a solid core.
  // Sized to the full viewBox — an inset glyph reads as smaller still once it
  // sits at 16px in a sidebar.
  return (
    <path d="M8 1 14.1 4.5v7L8 15 1.9 11.5v-7zM8 3.1 3.7 5.55v4.9L8 12.9l4.3-2.45v-4.9zM8 5.6a2.4 2.4 0 1 0 0 4.8 2.4 2.4 0 0 0 0-4.8z" />
  );
}

/** Anthropic's burst: an even radial star, filled so it survives small sizes. */
function ClaudeMark() {
  const spokes = Array.from({ length: 8 }, (_, index) => (index * 180) / 8);
  return (
    <g>
      {spokes.map((angle) => (
        <rect
          key={angle}
          x="7.05"
          y="1.2"
          width="1.9"
          height="13.6"
          rx="0.95"
          transform={`rotate(${angle} 8 8)`}
        />
      ))}
    </g>
  );
}

/** DeepSeek's whale, as a filled silhouette. */
function DeepSeekMark() {
  return (
    <path d="M1.6 8.4c2.9.5 5-.4 6.7-2.6.5-.7 1-1.4 1.7-1.9.2-.2.5 0 .4.3-.2.7-.3 1.4-.2 2 1.3.2 2.6.1 3.9-.3.3-.1.5.2.3.4-.6.7-.9 1.5-1.1 2.4-.5 2.2-2.4 3.7-4.9 3.7-3 0-5.6-1.5-6.9-3.6-.1-.2 0-.4.1-.4z" />
  );
}

/** Gemini's four-point spark. */
function GeminiMark() {
  return (
    <path d="M8 1c.3 3.6 3.4 6.7 7 7-3.6.3-6.7 3.4-7 7-.3-3.6-3.4-6.7-7-7 3.6-.3 6.7-3.4 7-7z" />
  );
}

/**
 * Anything unmapped: a plain endpoint.
 *
 * Deliberately the dullest of the set — a local model is not a lesser thing,
 * but it has no mark of its own and pretending otherwise would be inventing a
 * brand for it.
 */
function GenericMark() {
  return (
    <g>
      <rect x="2" y="3" width="12" height="4.4" rx="1.3" />
      <rect x="2" y="8.6" width="12" height="4.4" rx="1.3" />
      <circle cx="4.6" cy="5.2" r="0.85" className="text-background" fill="currentColor" />
      <circle cx="4.6" cy="10.8" r="0.85" className="text-background" fill="currentColor" />
    </g>
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
      // Filled, not stroked. A 1.4px outline at 12px is a grey smudge — solid
      // shapes are the only thing that reads at sidebar size.
      fill="currentColor"
      fillRule="evenodd"
      className={cn("size-4 shrink-0", className)}
      role={label ? "img" : undefined}
      aria-label={label}
      aria-hidden={label ? undefined : true}
    >
      {draw()}
    </svg>
  );
}

