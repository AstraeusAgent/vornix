import { useEffect } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { useChatStore } from "../features/chat/store";

function relativeTime(iso: string) {
  const diffMs = Date.now() - new Date(iso).getTime();
  const mins = Math.floor(diffMs / 60000);
  if (mins < 1) return "now";
  if (mins < 60) return `${mins}m`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h`;
  const days = Math.floor(hours / 24);
  if (days < 7) return `${days}d`;
  return new Date(iso).toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

export function Sidebar() {
  const sessions = useChatStore((s) => s.sessions);
  const activeSessionId = useChatStore((s) => s.activeSessionId);
  const fetchSessions = useChatStore((s) => s.fetchSessions);
  const selectSession = useChatStore((s) => s.selectSession);
  const deleteSession = useChatStore((s) => s.deleteSession);
  const createSession = useChatStore((s) => s.createSession);

  useEffect(() => {
    fetchSessions();
  }, []);

  return (
    <aside className="w-64 bg-vornix-surface border-r border-vornix-border flex flex-col shrink-0">
      <div className="p-3">
        <motion.button
          onClick={() => createSession()}
          whileTap={{ scale: 0.98 }}
          className="w-full px-3 py-2 bg-vornix-accent/10 hover:bg-vornix-accent/20 border border-vornix-accent/20 rounded-lg text-sm text-vornix-accent font-medium transition-colors flex items-center justify-center gap-1.5"
        >
          <span className="text-base leading-none">+</span> New Session
        </motion.button>
      </div>

      {sessions.length > 0 && (
        <div className="px-5 pb-1 pt-1 text-[10px] font-medium uppercase tracking-wider text-vornix-text-muted">
          Sessions
        </div>
      )}

      <div className="flex-1 overflow-y-auto px-2">
        <AnimatePresence initial={false}>
          {sessions.map((session) => (
            <motion.div
              key={session.id}
              layout
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0, height: 0, marginBottom: 0 }}
              transition={{ duration: 0.15 }}
              className={`group relative flex items-center gap-2 px-3 py-2 rounded-lg cursor-pointer text-sm mb-0.5 transition-colors ${
                activeSessionId === session.id
                  ? "bg-vornix-accent/10 text-vornix-text"
                  : "text-vornix-text-muted hover:bg-vornix-surface-hover hover:text-vornix-text"
              }`}
              onClick={() => selectSession(session.id)}
            >
              {activeSessionId === session.id && (
                <motion.span
                  layoutId="sidebar-active-indicator"
                  className="absolute left-0 top-1.5 bottom-1.5 w-0.5 rounded-full bg-vornix-accent"
                />
              )}
              <div className="flex-1 min-w-0">
                <div className="truncate">{session.title || "Untitled Session"}</div>
              </div>
              <span className="text-[10px] text-vornix-text-muted shrink-0 group-hover:hidden">
                {relativeTime(session.updated_at)}
              </span>
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  deleteSession(session.id);
                }}
                className="hidden group-hover:block text-vornix-text-muted hover:text-vornix-error text-xs shrink-0 transition-colors"
                aria-label="Delete session"
              >
                ✕
              </button>
            </motion.div>
          ))}
        </AnimatePresence>

        {sessions.length === 0 && (
          <p className="text-xs text-vornix-text-muted text-center mt-8 px-4 leading-relaxed">
            No sessions yet.
            <br />
            Create one to get started.
          </p>
        )}
      </div>
    </aside>
  );
}
