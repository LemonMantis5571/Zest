/** Bundled languages we highlight in chat / edit previews. */
const LANGS = [
  "typescript",
  "tsx",
  "javascript",
  "jsx",
  "python",
  "rust",
  "go",
  "java",
  "c",
  "cpp",
  "csharp",
  "json",
  "toml",
  "yaml",
  "markdown",
  "html",
  "css",
  "scss",
  "sql",
  "bash",
  "shellscript",
  "powershell",
  "diff",
  "plaintext",
] as const;

type Lang = (typeof LANGS)[number];

const ALIASES: Record<string, Lang> = {
  ts: "typescript",
  js: "javascript",
  py: "python",
  rs: "rust",
  sh: "bash",
  zsh: "bash",
  shell: "shellscript",
  ps1: "powershell",
  yml: "yaml",
  md: "markdown",
  text: "plaintext",
  txt: "plaintext",
  "": "plaintext",
};

type Highlighter = Awaited<
  ReturnType<typeof import("shiki").createHighlighter>
>;

let highlighterPromise: Promise<Highlighter> | null = null;

function getHighlighter(): Promise<Highlighter> {
  if (!highlighterPromise) {
    highlighterPromise = import("shiki").then(({ createHighlighter }) =>
      createHighlighter({
        themes: ["github-dark-default"],
        langs: [...LANGS],
      })
    );
  }
  return highlighterPromise;
}

export function normalizeLang(raw: string | undefined | null): Lang {
  const key = (raw ?? "").trim().toLowerCase();
  if ((LANGS as readonly string[]).includes(key)) return key as Lang;
  return ALIASES[key] ?? "plaintext";
}

export function languageLabel(lang: string): string {
  const n = normalizeLang(lang);
  if (n === "plaintext") return "text";
  if (n === "typescript") return "ts";
  if (n === "javascript") return "js";
  if (n === "shellscript") return "shell";
  if (n === "powershell") return "ps1";
  return n;
}

/** Highlight to HTML (no outer pre wrapper — we style the host). */
export async function highlightCode(
  code: string,
  langHint?: string | null
): Promise<string> {
  const lang = normalizeLang(langHint);
  const highlighter = await getHighlighter();
  return highlighter.codeToHtml(code, {
    lang,
    theme: "github-dark-default",
  });
}
