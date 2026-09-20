import { Fragment } from "react";
import { CodeBlock } from "./CodeBlock";

const FENCE_RE = /```([a-zA-Z0-9_+-]*)\n?([\s\S]*?)```/g;

function renderInline(text: string, keyPrefix: string) {
  const parts = text.split(/(`[^`\n]+`|\*\*[^*\n]+\*\*)/g).filter(Boolean);
  return parts.map((part, i) => {
    const key = `${keyPrefix}-${i}`;
    if (part.startsWith("`") && part.endsWith("`") && part.length > 1) {
      return (
        <code
          key={key}
          className="px-1 py-0.5 rounded bg-black/30 border border-vornix-border text-[0.85em] font-mono text-vornix-accent-hover"
        >
          {part.slice(1, -1)}
        </code>
      );
    }
    if (part.startsWith("**") && part.endsWith("**") && part.length > 3) {
      return (
        <strong key={key} className="font-semibold text-vornix-text">
          {part.slice(2, -2)}
        </strong>
      );
    }
    return <Fragment key={key}>{part}</Fragment>;
  });
}

export function MessageContent({ content }: { content: string }) {
  const nodes: React.ReactNode[] = [];
  let lastIndex = 0;
  let match: RegExpExecArray | null;
  let blockIndex = 0;

  FENCE_RE.lastIndex = 0;
  while ((match = FENCE_RE.exec(content)) !== null) {
    if (match.index > lastIndex) {
      const text = content.slice(lastIndex, match.index);
      if (text.trim()) {
        nodes.push(
          <p key={`t-${blockIndex}`} className="whitespace-pre-wrap">
            {renderInline(text.trim(), `t-${blockIndex}`)}
          </p>
        );
      }
    }
    const [, lang, code] = match;
    nodes.push(<CodeBlock key={`c-${blockIndex}`} lang={lang} code={code.replace(/\n$/, "")} />);
    lastIndex = FENCE_RE.lastIndex;
    blockIndex++;
  }

  if (lastIndex < content.length) {
    const text = content.slice(lastIndex);
    if (text.trim()) {
      nodes.push(
        <p key={`t-${blockIndex}-end`} className="whitespace-pre-wrap">
          {renderInline(text.trim(), `t-${blockIndex}-end`)}
        </p>
      );
    }
  }

  if (nodes.length === 0) {
    return <p className="whitespace-pre-wrap">{content}</p>;
  }

  return <div className="space-y-2">{nodes}</div>;
}
