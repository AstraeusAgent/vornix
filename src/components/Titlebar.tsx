export function Titlebar() {
  return (
    <div
      data-tauri-drag-region
      className="h-10 flex items-center justify-between px-4 border-b border-sable-border bg-sable-surface select-none shrink-0"
    >
      <div className="flex items-center gap-2">
        <span className="text-sm font-semibold text-sable-accent">Sable</span>
        <span className="text-xs text-sable-text-muted">v0.1.0</span>
      </div>
      <div className="flex items-center gap-3">
        <button className="text-xs text-sable-text-muted hover:text-sable-text transition-colors">
          Settings
        </button>
      </div>
    </div>
  );
}