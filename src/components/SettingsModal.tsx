import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AnimatePresence, motion } from "framer-motion";

interface SettingsModalProps {
  open: boolean;
  onClose: () => void;
}

type ProviderId = "openrouter" | "opencode-go";

interface KeyStatus {
  provider_id: string;
  masked: string | null;
  backend: string;
}

export function SettingsModal({ open, onClose }: SettingsModalProps) {
  const [provider, setProvider] = useState<ProviderId>("openrouter");
  const [apiKey, setApiKey] = useState("");
  const [status, setStatus] = useState<{ kind: "ok" | "err"; msg: string } | null>(null);
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);
  const [keyStatus, setKeyStatus] = useState<Record<string, KeyStatus>>({});

  const loadStatus = async () => {
    try {
      const results = await Promise.all(
        (["openrouter", "opencode-go"] as ProviderId[]).map((p) =>
          invoke<KeyStatus>("provider_key_status", { providerId: p })
        )
      );
      setKeyStatus(Object.fromEntries(results.map((r) => [r.provider_id, r])));
    } catch {
      // status display is best-effort
    }
  };

  useEffect(() => {
    if (open) loadStatus();
  }, [open]);

  const save = async () => {
    if (!apiKey.trim()) return;
    setSaving(true);
    setStatus(null);
    try {
      await invoke("provider_set_key", {
        providerId: provider,
        apiKey: apiKey.trim(),
      });
      setStatus({
        kind: "ok",
        msg: "Key saved permanently to your OS keychain. It survives app restarts.",
      });
      setApiKey("");
      await loadStatus();
    } catch (e: any) {
      setStatus({ kind: "err", msg: String(e) });
    } finally {
      setSaving(false);
    }
  };

  const removeKey = async () => {
    setStatus(null);
    try {
      await invoke("provider_delete_key", { providerId: provider });
      await loadStatus();
      setStatus({ kind: "ok", msg: "Key removed from keychain." });
    } catch (e: any) {
      setStatus({ kind: "err", msg: String(e) });
    }
  };

  const testConnection = async () => {
    setTesting(true);
    setStatus(null);
    try {
      const body = await invoke<string>("provider_list_models", { providerId: provider });
      const models = JSON.parse(body);
      const count = Array.isArray(models?.data) ? models.data.length : 0;
      setStatus({ kind: "ok", msg: `Connection OK — ${count} models available.` });
    } catch (e: any) {
      setStatus({ kind: "err", msg: String(e) });
    } finally {
      setTesting(false);
    }
  };

  return (
    <AnimatePresence>
      {open && (
        <motion.div
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.15 }}
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/60"
          onClick={onClose}
        >
          <motion.div
            initial={{ opacity: 0, scale: 0.96, y: 8 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.96, y: 8 }}
            transition={{ duration: 0.15, ease: "easeOut" }}
            className="w-[480px] bg-vornix-surface border border-vornix-border rounded-xl shadow-2xl"
            onClick={(e) => e.stopPropagation()}
          >
        <div className="flex items-center justify-between px-5 py-4 border-b border-vornix-border">
          <h2 className="text-sm font-semibold text-vornix-text">Settings</h2>
          <button
            onClick={onClose}
            className="text-vornix-text-muted hover:text-vornix-text text-sm"
            aria-label="Close settings"
          >
            ✕
          </button>
        </div>

        <div className="px-5 py-4 space-y-4">
          <div>
            <label className="block text-xs font-medium text-vornix-text-muted mb-2">
              Provider
            </label>
            <div className="flex gap-2">
              {(
                [
                  ["openrouter", "OpenRouter"],
                  ["opencode-go", "OpenCode Go"],
                ] as [ProviderId, string][]
              ).map(([id, label]) => (
                <button
                  key={id}
                  onClick={() => setProvider(id)}
                  className={`px-3 py-1.5 rounded-md text-xs font-medium transition-colors ${
                    provider === id
                      ? "bg-vornix-accent text-white"
                      : "bg-vornix-bg text-vornix-text-muted border border-vornix-border hover:text-vornix-text"
                  }`}
                >
                  {label}
                </button>
              ))}
            </div>
          </div>

          <div>
            <label className="block text-xs font-medium text-vornix-text-muted mb-2">
              API key
            </label>

            {keyStatus[provider]?.masked && (
              <div className="flex items-center justify-between gap-2 mb-2 px-3 py-2 rounded-md bg-vornix-success/10 border border-vornix-success/20">
                <span className="text-xs text-vornix-success font-mono">
                  Saved: {keyStatus[provider].masked}
                </span>
                <button
                  onClick={removeKey}
                  className="text-[10px] text-vornix-error/70 hover:text-vornix-error font-medium"
                >
                  Remove
                </button>
              </div>
            )}

            <input
              type="password"
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              placeholder={provider === "openrouter" ? "sk-or-v1-..." : "API key"}
              className="w-full bg-vornix-bg border border-vornix-border rounded-lg px-3 py-2 text-sm text-vornix-text placeholder:text-vornix-text-muted focus:outline-none focus:ring-1 focus:ring-vornix-accent"
            />
            <p className="text-xs text-vornix-text-muted mt-1.5">
              Stored in your OS keychain, never in plaintext on disk.
            </p>
          </div>

          <div className="flex gap-2">
            <button
              onClick={save}
              disabled={!apiKey.trim() || saving}
              className="px-4 py-1.5 bg-vornix-accent hover:bg-vornix-accent-hover disabled:opacity-40 text-white rounded-md text-xs font-medium transition-colors"
            >
              {saving ? "Saving..." : "Save key"}
            </button>
            <button
              onClick={testConnection}
              disabled={testing}
              className="px-4 py-1.5 bg-vornix-bg border border-vornix-border hover:border-vornix-text-muted disabled:opacity-40 text-vornix-text rounded-md text-xs font-medium transition-colors"
            >
              {testing ? "Testing..." : "Test connection"}
            </button>
          </div>

          {status && (
            <div
              className={`px-3 py-2 rounded-md text-xs ${
                status.kind === "ok"
                  ? "bg-vornix-success/10 text-vornix-success border border-vornix-success/20"
                  : "bg-vornix-error/10 text-vornix-error border border-vornix-error/20"
              }`}
            >
              {status.msg}
            </div>
          )}
        </div>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}