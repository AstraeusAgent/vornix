import { getCurrentWindow } from "@tauri-apps/api/window";

interface TitlebarProps {
  onOpenSettings: () => void;
}

export function Titlebar({ onOpenSettings }: TitlebarProps) {
  const appWindow = getCurrentWindow();

  return (
    <div
      data-tauri-drag-region
      className="h-10 flex items-center justify-between pl-4 border-b border-vornix-border bg-vornix-surface select-none shrink-0"
    >
      <div className="flex items-center gap-2" data-tauri-drag-region>
        <span className="w-1.5 h-1.5 rounded-full bg-vornix-accent shrink-0" />
        <span className="text-sm font-semibold tracking-tight text-vornix-text">Vornix</span>
        <span className="text-[10px] text-vornix-text-muted">v0.1.0</span>
      </div>

      <div className="flex items-center h-full">
        <button
          onClick={onOpenSettings}
          className="h-full px-4 text-xs text-vornix-text-muted hover:text-vornix-text hover:bg-vornix-surface-hover transition-colors"
        >
          Settings
        </button>
        <button
          onClick={() => appWindow.minimize()}
          className="h-full w-11 flex items-center justify-center text-vornix-text-muted hover:text-vornix-text hover:bg-vornix-surface-hover transition-colors"
          aria-label="Minimize"
        >
          <svg width="10" height="10" viewBox="0 0 10 10"><path d="M0 5h10" stroke="currentColor" strokeWidth="1" /></svg>
        </button>
        <button
          onClick={() => appWindow.toggleMaximize()}
          className="h-full w-11 flex items-center justify-center text-vornix-text-muted hover:text-vornix-text hover:bg-vornix-surface-hover transition-colors"
          aria-label="Maximize"
        >
          <svg width="10" height="10" viewBox="0 0 10 10"><rect x="0.5" y="0.5" width="9" height="9" fill="none" stroke="currentColor" strokeWidth="1" /></svg>
        </button>
        <button
          onClick={() => appWindow.close()}
          className="h-full w-11 flex items-center justify-center text-vornix-text-muted hover:text-white hover:bg-red-600 transition-colors"
          aria-label="Close"
        >
          <svg width="10" height="10" viewBox="0 0 10 10"><path d="M0 0l10 10M10 0L0 10" stroke="currentColor" strokeWidth="1" /></svg>
        </button>
      </div>
    </div>
  );
}