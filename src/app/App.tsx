import { useChatStore } from "../features/chat/store";
import { Titlebar } from "../components/Titlebar";
import { Sidebar } from "../components/Sidebar";
import { ChatView } from "../features/chat/ChatView";

export default function App() {
  const activeSessionId = useChatStore((s) => s.activeSessionId);

  return (
    <div className="flex flex-col h-screen w-screen bg-sable-bg">
      <Titlebar />
      <div className="flex flex-1 overflow-hidden">
        <Sidebar />
        <main className="flex-1 flex flex-col overflow-hidden">
          {activeSessionId ? (
            <ChatView sessionId={activeSessionId} />
          ) : (
            <EmptyState />
          )}
        </main>
      </div>
    </div>
  );
}

function EmptyState() {
  const createSession = useChatStore((s) => s.createSession);

  return (
    <div className="flex-1 flex items-center justify-center">
      <div className="text-center max-w-md">
        <div className="text-5xl mb-4 font-bold text-sable-accent tracking-tight">
          Sable
        </div>
        <p className="text-sable-text-muted mb-8 text-sm leading-relaxed">
          A debugging-first coding agent with a voice. Not a compliance bot — a sharp, slightly paranoid partner.
        </p>
        <button
          onClick={() => createSession()}
          className="px-6 py-3 bg-sable-accent hover:bg-sable-accent-hover text-white rounded-lg text-sm font-medium transition-colors"
        >
          New Session
        </button>
      </div>
    </div>
  );
}