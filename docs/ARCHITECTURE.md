# Sable — Architecture

Sable is a desktop AI coding harness built with Tauri v2 (Rust core) and React/TypeScript (frontend).

## Directory Structure

```
sable/
├── src-tauri/                    # Rust workspace
│   ├── Cargo.toml                # Workspace root + sable-app binary crate
│   ├── tauri.conf.json            # Tauri configuration
│   ├── capabilities/              # IPC permission scopes
│   ├── src/                       # sable-app binary source
│   │   ├── main.rs
│   │   ├── lib.rs                 # Tauri setup, vault/memory/persona init
│   │   ├── commands.rs            # Tauri IPC commands
│   │   └── state.rs               # AppState (vault, sessions, memory, mcp, persona)
│   └── crates/                    # Library crates
│       ├── sable-core/            # Agent loop: state machine, orchestrator, context builder
│       ├── sable-providers/       # Provider trait, OpenRouter + OpenCode Go clients
│       ├── sable-mcp/             # MCP client manager, JSON-RPC over stdio
│       ├── sable-tools/           # Tool schemas, permission gate, registry, undo
│       ├── sable-skills/          # SKILL.md loader, index, progressive disclosure
│       ├── sable-memory/          # Session store, long-term memory, compaction
│       ├── sable-git/             # git2-rs engine, GitHub device flow
│       ├── sable-secrets/         # OS keychain + age-encrypted fallback
│       └── sable-persona/         # Persona policy engine (Off/Subtle/Full)
├── src/                          # Frontend (React 19 + TypeScript)
│   ├── main.tsx
│   ├── app/App.tsx
│   ├── features/chat/
│   │   ├── store.ts               # Zustand store (sessions, messages, send)
│   │   └── ChatView.tsx            # Chat UI
│   ├── components/
│   │   ├── Titlebar.tsx            # Custom window titlebar
│   │   └── Sidebar.tsx             # Session list
│   └── styles/global.css          # Tailwind v4, design tokens
├── mcp-servers/                  # Bundled MCP servers (Node.js, JSON-RPC 2.0)
│   ├── sable-fs/                  # Filesystem: read, write, list, glob, info
│   ├── sable-shell/               # Shell: execute, start/list/kill processes
│   └── sable-thinking/            # Sequential thinking scratchpad
└── docs/
    ├── ARCHITECTURE.md
    └── STATUS.md                  # Living status ledger
```

## Key Design Decisions

1. **Trait-injected provider system** — `ChatProvider` and `ToolExecutor` are traits in `sable-core`. The orchestrator is decoupled from specific LLM backends.

2. **MCP-all-the-way-down** — every tool (including bundled filesystem/shell) speaks JSON-RPC 2.0 over stdio. Adding a new capability = connecting a new MCP server.

3. **Permission tier enforcement in Rust** — Tier classification and approval gating happen in the Rust core, not in the system prompt. A compromised model cannot bypass the permission gate.

4. **Persona as policy, not prompt** — behavioral rules (verify-before-done, read-before-edit) are enforced by the orchestrator's state machine, not just suggested in the prompt. The persona gives the *reason*, the loop gives the *enforcement*.

5. **SQLite everything** — sessions, messages, tool call logs, and long-term memory all live in a single SQLite database with WAL mode and per-message transactions for crash safety.

6. **OS keychain for secrets** — API keys and tokens are stored via the native keyring (macOS Keychain, Windows Credential Manager, Linux Secret Service). If no keyring is available, falls back to age-encrypted file — never plaintext.