import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

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

  if (!open) return null;

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
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60"
      onClick={onClose}
    >
      <div
        className="w-[480px] bg-sable-surface border border-sable-border rounded-xl shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between px-5 py-4 border-b border-sable-border">
          <h2 className="text-sm font-semibold text-sable-text">Settings</h2>
          <button
            onClick={onClose}
            className="text-sable-text-muted hover:text-sable-text text-sm"
            aria-label="Close settings"
          >
            ✕
          </button>
        </div>

        <div className="px-5 py-4 space-y-4">
          <div>
            <label className="block text-xs font-medium text-sable-text-muted mb-2">
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
                      ? "bg-sable-accent text-white"
                      : "bg-sable-bg text-sable-text-muted border border-sable-border hover:text-sable-text"
                  }`}
                >
                  {label}
                </button>
              ))}
            </div>
          </div>

          <div>
            <label className="block text-xs font-medium text-sable-text-muted mb-2">
              API key
            </label>

            {keyStatus[provider]?.masked && (
              <div className="flex items-center justify-between gap-2 mb-2 px-3 py-2 rounded-md bg-sable-success/10 border border-sable-success/20">
                <span className="text-xs text-sable-success font-mono">
                  Saved: {keyStatus[provider].masked}
                </span>
                <button
                  onClick={removeKey}
                  className="text-[10px] text-sable-error/70 hover:text-sable-error font-medium"
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
              className="w-full bg-sable-bg border border-sable-border rounded-lg px-3 py-2 text-sm text-sable-text placeholder:text-sable-text-muted focus:outline-none focus:ring-1 focus:ring-sable-accent"
            />
            <p className="text-xs text-sable-text-muted mt-1.5">
              Stored in your OS keychain, never in plaintext on disk.
            </p>
          </div>

          <div className="flex gap-2">
            <button
              onClick={save}
              disabled={!apiKey.trim() || saving}
              className="px-4 py-1.5 bg-sable-accent hover:bg-sable-accent-hover disabled:opacity-40 text-white rounded-md text-xs font-medium transition-colors"
            >
              {saving ? "Saving..." : "Save key"}
            </button>
            <button
              onClick={testConnection}
              disabled={testing}
              className="px-4 py-1.5 bg-sable-bg border border-sable-border hover:border-sable-text-muted disabled:opacity-40 text-sable-text rounded-md text-xs font-medium transition-colors"
            >
              {testing ? "Testing..." : "Test connection"}
            </button>
          </div>

          {status && (
            <div
              className={`px-3 py-2 rounded-md text-xs ${
                status.kind === "ok"
                  ? "bg-sable-success/10 text-sable-success border border-sable-success/20"
                  : "bg-sable-error/10 text-sable-error border border-sable-error/20"
              }`}
            >
              {status.msg}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}