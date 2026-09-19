import { useEffect, useRef, useState } from "react";
import { useChatStore } from "./store";

export function ChatView({ sessionId }: { sessionId: string }) {
  const messages = useChatStore((s) => s.messages[sessionId] || []);
  const isLoading = useChatStore((s) => s.isLoading);
  const error = useChatStore((s) => s.error);
  const sendMessage = useChatStore((s) => s.sendMessage);
  const [input, setInput] = useState("");
  const messagesEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  const handleSend = () => {
    const trimmed = input.trim();
    if (!trimmed || isLoading) return;
    setInput("");
    sendMessage(sessionId, trimmed);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  return (
    <div className="flex flex-col flex-1 overflow-hidden">
      {/* Messages */}
      <div className="flex-1 overflow-y-auto px-4 py-6">
        <div className="max-w-3xl mx-auto space-y-6">
          {messages.map((msg) => (
            <MessageBubble key={msg.id} role={msg.role} content={msg.content} />
          ))}
          {isLoading && (
            <div className="flex items-center gap-2 text-sable-text-muted text-sm">
              <span className="inline-block w-2 h-2 rounded-full bg-sable-accent animate-pulse" />
              Thinking...
            </div>
          )}
          {error && (
            <div className="px-4 py-3 rounded-lg bg-sable-error/10 border border-sable-error/20 text-sable-error text-sm">
              {error}
            </div>
          )}
          <div ref={messagesEndRef} />
        </div>
      </div>

      {/* Composer */}
      <div className="border-t border-sable-border p-4">
        <div className="max-w-3xl mx-auto">
          <div className="relative">
            <textarea
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="Ask Sable anything..."
              rows={1}
              className="w-full bg-sable-surface border border-sable-border rounded-lg px-4 py-3 pr-24 text-sm text-sable-text placeholder:text-sable-text-muted resize-none focus:outline-none focus:ring-1 focus:ring-sable-accent focus:border-sable-accent"
            />
            <button
              onClick={handleSend}
              disabled={!input.trim() || isLoading}
              className="absolute right-2 top-1/2 -translate-y-1/2 px-4 py-1.5 bg-sable-accent hover:bg-sable-accent-hover disabled:opacity-40 disabled:cursor-not-allowed text-white rounded-md text-xs font-medium transition-colors"
            >
              Send
            </button>
          </div>
          <p className="text-xs text-sable-text-muted mt-2">
            {messages.length} messages this session
          </p>
        </div>
      </div>
    </div>
  );
}

function MessageBubble({ role, content }: { role: string; content: string }) {
  const isUser = role === "user";

  return (
    <div className={`flex ${isUser ? "justify-end" : "justify-start"}`}>
      <div
        className={`max-w-[85%] rounded-lg px-4 py-3 text-sm leading-relaxed whitespace-pre-wrap ${
          isUser
            ? "bg-sable-accent/15 border border-sable-accent/20 text-sable-text"
            : "bg-sable-surface border border-sable-border text-sable-text"
        }`}
      >
        {!isUser && (
          <div className="text-xs font-medium text-sable-accent mb-2">Sable</div>
        )}
        {content}
      </div>
    </div>
  );
}