import { useEffect, useState } from "react";
import type { Highlighter } from "shiki";

let highlighterPromise: Promise<Highlighter> | null = null;
const loadedLangs = new Set<string>(["text"]);

async function getHighlighter() {
  if (!highlighterPromise) {
    highlighterPromise = import("shiki").then((shiki) =>
      shiki.createHighlighter({
        themes: ["github-dark-default"],
        langs: ["text"],
      })
    );
  }
  return highlighterPromise;
}

async function ensureLang(lang: string) {
  const highlighter = await getHighlighter();
  if (loadedLangs.has(lang)) return highlighter;
  try {
    await highlighter.loadLanguage(lang as any);
    loadedLangs.add(lang);
  } catch {
    // unknown language id — fall back to plain text highlighting
  }
  return highlighter;
}

export function CodeBlock({ code, lang }: { code: string; lang: string }) {
  const [html, setHtml] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const normalizedLang = lang.trim().toLowerCase() || "text";

  useEffect(() => {
    let cancelled = false;
    ensureLang(normalizedLang).then((highlighter) => {
      if (cancelled) return;
      const known = highlighter.getLoadedLanguages().includes(normalizedLang as any);
      setHtml(
        highlighter.codeToHtml(code, {
          lang: known ? normalizedLang : "text",
          theme: "github-dark-default",
        })
      );
    });
    return () => {
      cancelled = true;
    };
  }, [code, normalizedLang]);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(code);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // clipboard unavailable — ignore
    }
  };

  return (
    <div className="relative group/code my-2 rounded-lg overflow-hidden border border-vornix-border bg-[#0d1117]">
      <div className="flex items-center justify-between px-3 py-1.5 bg-black/20 border-b border-vornix-border">
        <span className="text-[10px] uppercase tracking-wide text-vornix-text-muted">
          {normalizedLang}
        </span>
        <button
          onClick={copy}
          className="text-[10px] text-vornix-text-muted hover:text-vornix-text transition-colors opacity-0 group-hover/code:opacity-100"
        >
          {copied ? "Copied" : "Copy"}
        </button>
      </div>
      {html ? (
        <div
          className="text-xs leading-relaxed overflow-x-auto [&_pre]:p-3 [&_pre]:m-0 [&_pre]:bg-transparent!"
          dangerouslySetInnerHTML={{ __html: html }}
        />
      ) : (
        <pre className="text-xs leading-relaxed overflow-x-auto p-3 m-0 text-vornix-text font-mono">
          {code}
        </pre>
      )}
    </div>
  );
}
