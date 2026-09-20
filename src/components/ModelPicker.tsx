import { useEffect, useMemo, useRef, useState } from "react";
import { useChatStore } from "../features/chat/store";

export function ModelPicker() {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [providerFilter, setProviderFilter] = useState<string>("openrouter");

  const models = useChatStore((s) => s.models);
  const modelsProvider = useChatStore((s) => s.modelsProvider);
  const modelsLoading = useChatStore((s) => s.modelsLoading);
  const fetchModels = useChatStore((s) => s.fetchModels);
  const selectedModel = useChatStore((s) => s.selectedModel);
  const setSelectedModel = useChatStore((s) => s.setSelectedModel);
  const searchRef = useRef<HTMLInputElement>(null);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    fetchModels(providerFilter);
  }, [providerFilter]);

  useEffect(() => {
    if (open) {
      setTimeout(() => searchRef.current?.focus(), 10);
      const onClick = (e: MouseEvent) => {
        if (rootRef.current && !rootRef.current.contains(e.target as Node)) {
          setOpen(false);
        }
      };
      document.addEventListener("mousedown", onClick);
      return () => document.removeEventListener("mousedown", onClick);
    }
  }, [open]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return models;
    return models.filter(
      (m) =>
        m.id.toLowerCase().includes(q) || m.name.toLowerCase().includes(q)
    );
  }, [models, query]);

  const selected = models.find((m) => m.id === selectedModel);

  const formatPrice = (m: (typeof models)[number]) => {
    if (m.prompt_price == null) return "subscription";
    if (m.prompt_price === 0 && m.completion_price === 0) return "free";
    return `$${m.prompt_price.toFixed(2)}/$${(m.completion_price ?? 0).toFixed(2)} per Mtok`;
  };

  return (
    <div ref={rootRef} className="relative">
      <button
        onClick={() => setOpen((o) => !o)}
        className="flex items-center gap-2 px-3 py-1.5 rounded-md bg-vornix-bg border border-vornix-border hover:border-vornix-text-muted text-xs text-vornix-text transition-colors max-w-[260px]"
      >
        <span className="w-1.5 h-1.5 rounded-full bg-vornix-accent shrink-0" />
        <span className="truncate">
          {selected ? selected.name : selectedModel ?? "Select model"}
        </span>
        <svg width="8" height="8" viewBox="0 0 8 8" className="shrink-0 text-vornix-text-muted">
          <path d="M1 3l3 3 3-3" fill="none" stroke="currentColor" strokeWidth="1" />
        </svg>
      </button>

      {open && (
        <div className="absolute bottom-full left-0 mb-2 w-[380px] bg-vornix-surface border border-vornix-border rounded-lg shadow-2xl z-40 overflow-hidden">
          <div className="flex items-center border-b border-vornix-border">
            {(
              [
                ["openrouter", "OpenRouter"],
                ["opencode-go", "OpenCode Go"],
              ] as [string, string][]
            ).map(([id, label]) => (
              <button
                key={id}
                onClick={() => setProviderFilter(id)}
                className={`px-3 py-2 text-xs font-medium transition-colors ${
                  providerFilter === id
                    ? "text-vornix-accent border-b border-vornix-accent"
                    : "text-vornix-text-muted hover:text-vornix-text"
                }`}
              >
                {label}
              </button>
            ))}
          </div>

          <div className="p-2 border-b border-vornix-border">
            <input
              ref={searchRef}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search models..."
              className="w-full bg-vornix-bg border border-vornix-border rounded-md px-3 py-1.5 text-xs text-vornix-text placeholder:text-vornix-text-muted focus:outline-none focus:ring-1 focus:ring-vornix-accent"
            />
          </div>

          <div className="max-h-[320px] overflow-y-auto">
            {modelsLoading && (
              <p className="px-3 py-4 text-xs text-vornix-text-muted text-center">
                Loading models...
              </p>
            )}
            {!modelsLoading && filtered.length === 0 && (
              <p className="px-3 py-4 text-xs text-vornix-text-muted text-center">
                No models match "{query}"
              </p>
            )}
            {filtered.map((m) => (
              <button
                key={m.id}
                onClick={() => {
                  setSelectedModel(m.id);
                  setOpen(false);
                  setQuery("");
                }}
                className={`w-full text-left px-3 py-2 hover:bg-vornix-surface-hover transition-colors ${
                  selectedModel === m.id ? "bg-vornix-accent/10" : ""
                }`}
              >
                <div className="flex items-center justify-between gap-2">
                  <span className="text-xs font-medium text-vornix-text truncate">
                    {m.name}
                  </span>
                  <span className="text-[10px] text-vornix-text-muted shrink-0">
                    {m.context_length > 0
                      ? `${Math.round(m.context_length / 1000)}k ctx`
                      : "ctx ?"}
                  </span>
                </div>
                <div className="flex items-center gap-2 mt-0.5">
                  <span className="text-[10px] text-vornix-text-muted truncate">
                    {formatPrice(m)}
                  </span>
                  {m.supports_reasoning && (
                    <span className="text-[9px] px-1 py-px rounded bg-vornix-accent/15 text-vornix-accent">
                      reasoning
                    </span>
                  )}
                  {m.supports_tools && (
                    <span className="text-[9px] px-1 py-px rounded bg-vornix-success/15 text-vornix-success">
                      tools
                    </span>
                  )}
                </div>
              </button>
            ))}
          </div>

          <div className="px-3 py-1.5 border-t border-vornix-border text-[10px] text-vornix-text-muted">
            {modelsProvider && models.length > 0 && `${filtered.length} of ${models.length} models`}
          </div>
        </div>
      )}
    </div>
  );
}