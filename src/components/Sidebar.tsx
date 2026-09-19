import { useEffect } from "react";
import { useChatStore } from "../features/chat/store";

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
    <aside className="w-64 bg-sable-surface border-r border-sable-border flex flex-col shrink-0">
      <div className="p-3">
        <button
          onClick={() => createSession()}
          className="w-full px-3 py-2 bg-sable-accent/10 hover:bg-sable-accent/20 border border-sable-accent/20 rounded-lg text-sm text-sable-accent font-medium transition-colors"
        >
          + New Session
        </button>
      </div>

      <div className="flex-1 overflow-y-auto px-2">
        {sessions.map((session) => (
          <div
            key={session.id}
            className={`group flex items-center gap-2 px-3 py-2 rounded-lg cursor-pointer text-sm mb-0.5 transition-colors ${
              activeSessionId === session.id
                ? "bg-sable-accent/10 text-sable-text"
                : "text-sable-text-muted hover:bg-sable-surface-hover hover:text-sable-text"
            }`}
            onClick={() => selectSession(session.id)}
          >
            <div className="flex-1 truncate">
              {session.title || "Untitled Session"}
            </div>
            <button
              onClick={(e) => {
                e.stopPropagation();
                deleteSession(session.id);
              }}
              className="opacity-0 group-hover:opacity-100 text-sable-text-muted hover:text-sable-error text-xs transition-all"
            >
              x
            </button>
          </div>
        ))}

        {sessions.length === 0 && (
          <p className="text-xs text-sable-text-muted text-center mt-8 px-4">
            No sessions yet. Create one to get started.
          </p>
        )}
      </div>
    </aside>
  );
}