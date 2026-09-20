import { useEffect, useRef, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { useChatStore, type Message } from "./store";
import { ModelPicker } from "../../components/ModelPicker";
import { MessageContent } from "../../components/MessageContent";

const MAX_TEXTAREA_HEIGHT = 200;

export function ChatView({ sessionId }: { sessionId: string }) {
  const EMPTY: Message[] = [];
  const messages = useChatStore((s) => s.messages[sessionId]) ?? EMPTY;
  const isLoading = useChatStore((s) => s.isLoading);
  const error = useChatStore((s) => s.error);
  const sendMessage = useChatStore((s) => s.sendMessage);
  const clearError = useChatStore((s) => s.clearError);
  const selectedModel = useChatStore((s) => s.selectedModel);
  const [input, setInput] = useState("");
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, isLoading]);

  useEffect(() => {
    const el = textareaRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, MAX_TEXTAREA_HEIGHT)}px`;
  }, [input]);

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
        <div className="max-w-3xl mx-auto space-y-5">
          <AnimatePresence initial={false}>
            {messages.map((msg) => (
              <MessageBubble key={msg.id} role={msg.role} content={msg.content} />
            ))}
          </AnimatePresence>

          {isLoading && (
            <motion.div
              initial={{ opacity: 0, y: 6 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0 }}
              className="flex items-center gap-3"
            >
              <Avatar role="assistant" />
              <ThinkingDots />
            </motion.div>
          )}

          <AnimatePresence>
            {error && (
              <motion.div
                initial={{ opacity: 0, y: 6 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: -6 }}
                className="px-4 py-3 rounded-lg bg-vornix-error/10 border border-vornix-error/20 text-vornix-error text-sm flex items-start justify-between gap-3"
              >
                <span className="whitespace-pre-wrap">{error}</span>
                <button
                  onClick={clearError}
                  className="shrink-0 text-vornix-error/60 hover:text-vornix-error"
                  aria-label="Dismiss error"
                >
                  ✕
                </button>
              </motion.div>
            )}
          </AnimatePresence>
          <div ref={messagesEndRef} />
        </div>
      </div>

      {/* Composer */}
      <div className="border-t border-vornix-border p-4 bg-vornix-bg/80 backdrop-blur-sm">
        <div className="max-w-3xl mx-auto">
          <div className="flex items-center gap-2 mb-2">
            <ModelPicker />
            <span className="text-[10px] text-vornix-text-muted truncate">
              {selectedModel ?? "no model selected"}
            </span>
          </div>
          <div className="relative rounded-lg border border-vornix-border bg-vornix-surface focus-within:border-vornix-accent focus-within:ring-1 focus-within:ring-vornix-accent transition-colors">
            <textarea
              ref={textareaRef}
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="Ask Vornix anything..."
              rows={1}
              className="w-full bg-transparent px-4 py-3 pr-20 text-sm text-vornix-text placeholder:text-vornix-text-muted resize-none focus:outline-none"
            />
            <motion.button
              onClick={handleSend}
              disabled={!input.trim() || isLoading}
              whileTap={{ scale: 0.95 }}
              className="absolute right-2 bottom-2.5 px-4 py-1.5 bg-vornix-accent hover:bg-vornix-accent-hover disabled:opacity-40 disabled:cursor-not-allowed text-white rounded-md text-xs font-medium transition-colors"
            >
              Send
            </motion.button>
          </div>
          <p className="text-xs text-vornix-text-muted mt-2">
            {messages.length} message{messages.length === 1 ? "" : "s"} this session
          </p>
        </div>
      </div>
    </div>
  );
}

function Avatar({ role }: { role: string }) {
  const isUser = role === "user";
  return (
    <div
      className={`shrink-0 w-7 h-7 rounded-full flex items-center justify-center text-[10px] font-semibold ${
        isUser
          ? "bg-vornix-surface-hover text-vornix-text-muted border border-vornix-border"
          : "bg-vornix-accent/15 text-vornix-accent border border-vornix-accent/30"
      }`}
    >
      {isUser ? "Y" : "S"}
    </div>
  );
}

function ThinkingDots() {
  return (
    <div className="flex items-center gap-1 px-4 py-3 rounded-lg bg-vornix-surface border border-vornix-border">
      {[0, 1, 2].map((i) => (
        <motion.span
          key={i}
          className="w-1.5 h-1.5 rounded-full bg-vornix-text-muted"
          animate={{ opacity: [0.3, 1, 0.3] }}
          transition={{ duration: 1.1, repeat: Infinity, delay: i * 0.15 }}
        />
      ))}
    </div>
  );
}

function MessageBubble({ role, content }: { role: string; content: string }) {
  const isUser = role === "user";

  return (
    <motion.div
      layout
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0 }}
      transition={{ duration: 0.18, ease: "easeOut" }}
      className={`flex gap-3 ${isUser ? "justify-end" : "justify-start"}`}
    >
      {!isUser && <Avatar role={role} />}
      <div
        className={`max-w-[85%] rounded-lg px-4 py-3 text-sm leading-relaxed ${
          isUser
            ? "bg-vornix-accent/15 border border-vornix-accent/20 text-vornix-text"
            : "bg-vornix-surface border border-vornix-border text-vornix-text"
        }`}
      >
        {!isUser && (
          <div className="text-xs font-medium text-vornix-accent mb-2">Vornix</div>
        )}
        <MessageContent content={content} />
      </div>
      {isUser && <Avatar role={role} />}
    </motion.div>
  );
}
