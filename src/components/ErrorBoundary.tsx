import { Component, type ReactNode } from "react";

interface Props {
  children: ReactNode;
}

interface State {
  error: Error | null;
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: { componentStack: string }) {
    console.error("Vornix crashed:", error, info.componentStack);
  }

  render() {
    if (this.state.error) {
      return (
        <div className="h-screen w-screen bg-vornix-bg flex items-center justify-center p-8">
          <div className="max-w-2xl w-full bg-vornix-surface border border-vornix-error/40 rounded-xl p-6">
            <h1 className="text-lg font-semibold text-vornix-error mb-2">
              Vornix hit a render error
            </h1>
            <p className="text-xs text-vornix-text-muted mb-4">
              The UI crashed. The error below is what broke it — reload to recover.
            </p>
            <pre className="text-xs text-vornix-error bg-vornix-bg rounded-lg p-4 overflow-auto max-h-[50vh] whitespace-pre-wrap">
              {this.state.error.message}
              {"\n\n"}
              {this.state.error.stack}
            </pre>
            <button
              onClick={() => {
                this.setState({ error: null });
                window.location.reload();
              }}
              className="mt-4 px-4 py-2 bg-vornix-accent hover:bg-vornix-accent-hover text-white rounded-md text-sm font-medium"
            >
              Reload
            </button>
          </div>
        </div>
      );
    }
    return this.props.children;
  }
}