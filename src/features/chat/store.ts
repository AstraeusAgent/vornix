import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";

export interface Session {
  id: string;
  title: string | null;
  model_id: string | null;
  created_at: string;
  updated_at: string;
}

export interface Message {
  id: string;
  role: string;
  content: string;
  timestamp: string;
}

export interface ModelSummary {
  id: string;
  name: string;
  provider_id: string;
  context_length: number;
  prompt_price: number | null;
  completion_price: number | null;
  supports_reasoning: boolean;
  supports_tools: boolean;
}

interface ChatState {
  sessions: Session[];
  activeSessionId: string | null;
  messages: Record<string, Message[]>;
  isLoading: boolean;
  error: string | null;
  models: ModelSummary[];
  modelsProvider: string | null;
  modelsLoading: boolean;
  selectedModel: string | null;

  fetchSessions: () => Promise<void>;
  createSession: (title?: string) => Promise<string>;
  selectSession: (id: string) => Promise<void>;
  deleteSession: (id: string) => Promise<void>;
  fetchMessages: (sessionId: string) => Promise<void>;
  sendMessage: (sessionId: string, content: string) => Promise<void>;
  clearError: () => void;
  fetchModels: (providerId: string) => Promise<void>;
  setSelectedModel: (modelId: string) => void;
}

export const useChatStore = create<ChatState>((set, get) => ({
  sessions: [],
  activeSessionId: null,
  messages: {},
  isLoading: false,
  error: null,
  models: [],
  modelsProvider: null,
  modelsLoading: false,
  selectedModel: (() => {
    try {
      return localStorage.getItem("vornix.lastModel");
    } catch {
      return null;
    }
  })(),

  fetchSessions: async () => {
    try {
      const sessions = await invoke<Session[]>("session_list", {
        limit: 50,
        offset: 0,
      });
      set({ sessions });
    } catch (e: any) {
      set({ error: e.toString() });
    }
  },

  createSession: async (title?: string) => {
    try {
      const session = await invoke<Session>("session_create", {
        title: title ?? null,
        modelId: null,
        providerId: null,
        projectPath: null,
      });
      set((s) => ({
        sessions: [session, ...s.sessions],
        activeSessionId: session.id,
      }));
      return session.id;
    } catch (e: any) {
      set({ error: e.toString() });
      throw e;
    }
  },

  selectSession: async (id: string) => {
    set({ activeSessionId: id, error: null });
    await get().fetchMessages(id);
  },

  deleteSession: async (id: string) => {
    try {
      await invoke("session_delete", { id });
      set((s) => ({
        sessions: s.sessions.filter((sess) => sess.id !== id),
        activeSessionId: s.activeSessionId === id ? null : s.activeSessionId,
      }));
    } catch (e: any) {
      set({ error: e.toString() });
    }
  },

  fetchMessages: async (sessionId: string) => {
    try {
      const messages = await invoke<Message[]>("session_get_messages", {
        sessionId,
        limit: 200,
        offset: 0,
      });
      set((s) => ({
        messages: { ...s.messages, [sessionId]: messages },
      }));
    } catch (e: any) {
      set({ error: e.toString() });
    }
  },

  sendMessage: async (sessionId: string, content: string) => {
    const tempId = `temp-${Date.now()}`;
    const userMsg: Message = {
      id: tempId,
      role: "user",
      content,
      timestamp: new Date().toISOString(),
    };

    // Optimistically add user message
    set((s) => ({
      messages: {
        ...s.messages,
        [sessionId]: [...(s.messages[sessionId] || []), userMsg],
      },
      isLoading: true,
      error: null,
    }));

    try {
      const resp = await invoke<{ content: string; reasoning: string | null; tokens_used: number }>(
        "chat_send_message",
        {
          sessionId,
          message: content,
          model: get().selectedModel,
        }
      );

      const assistantMsg: Message = {
        id: `assistant-${Date.now()}`,
        role: "assistant",
        content: resp.content,
        timestamp: new Date().toISOString(),
      };

      set((s) => ({
        messages: {
          ...s.messages,
          [sessionId]: [...(s.messages[sessionId] || []), assistantMsg],
        },
        isLoading: false,
      }));
    } catch (e: any) {
      set({ isLoading: false, error: e.toString() });
    }
  },

  clearError: () => set({ error: null }),

  fetchModels: async (providerId: string) => {
    if (get().modelsProvider === providerId && get().models.length > 0) return;
    set({ modelsLoading: true });
    try {
      const models = await invoke<ModelSummary[]>("list_models", {
        providerId,
      });
      set({ models, modelsProvider: providerId, modelsLoading: false });
    } catch (e: any) {
      set({ error: e.toString(), modelsLoading: false });
    }
  },

  setSelectedModel: (modelId: string) => {
    set({ selectedModel: modelId });
    try {
      localStorage.setItem("vornix.lastModel", modelId);
    } catch {
      // storage unavailable — session-only persistence
    }
  },
}));