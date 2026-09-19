# Sable — STATUS.md

> Living ledger of what's implemented and verified vs. designed-but-not-yet-built.

## Phase 1: Foundation ✅

| Component | Status | Notes |
|---|---|---|
| Tauri v2 shell | ✅ Built | Custom chrome, capability-scoped IPC |
| Secrets vault | ✅ Built + 10 tests | OS keychain + encrypted file fallback (age) |
| SQLite session store | ✅ Built + 6 tests | WAL-mode, full CRUD, tool call logging |
| Memory store | ✅ Built + 8 tests | SQLite-backed, text search, user-editable |
| Basic chat | ✅ Built | OpenRouter integration, non-streaming |

## Phase 2: Tool Core ✅

| Component | Status | Notes |
|---|---|---|
| sable-fs MCP server | ✅ Built | Real JSON-RPC 2.0 over stdio, 5 tools |
| sable-shell MCP server | ✅ Built | Real JSON-RPC 2.0 over stdio, 5 tools |
| sable-thinking MCP server | ✅ Built | Real JSON-RPC 2.0 over stdio, 3 tools |
| Permission gate | ✅ Built + 12 tests | 3-tier system, shell allowlist/destructive patterns |
| Tool registry | ✅ Built + 6 tests | Namespace-aware MCP discovery |
| Undo history | ✅ Built + 6 tests | Per-file, per-tool-call snapshots |

## Phase 3: Agent Loop ✅

| Component | Status | Notes |
|---|---|---|
| State machine | ✅ Built + 8 tests | 8 states, 14 valid transitions, iteration tracking |
| Loop detector | ✅ Built + 8 tests | Sliding window hash-based repetition detection |
| Context builder | ✅ Built + 6 tests | tiktoken-based token counting, ChatRequest assembly |
| Turn tracker | ✅ Built + 5 tests | Per-turn message/tool-call/timing/usage capture |
| Orchestrator | ✅ Built + 3 tests | Trait-injected ChatProvider/ToolExecutor, event bus |

## Phase 4: Debugging Subsystem ✅

| Component | Status | Notes |
|---|---|---|
| Hypothesis tracking | ✅ Built (via sable-thinking) | Structured thought chain with revisions |
| Reproduce-before-fix | ✅ Enforced by orchestrator | Observing state gate |
| Loop detector integration | ✅ Wired | Suspicious/Stuck → strategy change injection |

## Phase 5: Provider Integration ✅

| Component | Status | Notes |
|---|---|---|
| Provider trait | ✅ Defined | list_models, stream_chat, capabilities, health_check |
| OpenRouter client | ✅ API defined | /models, /chat/completions, supported_parameters parsing |
| OpenCode Go client | ✅ API defined | Separate provider with its own base URL |
| Model picker | ✅ Frontend stub | Command palette structure in place |

## Phase 6: Skills + Memory ✅

| Component | Status | Notes |
|---|---|---|
| SKILL.md format | ✅ Built + 3 tests | YAML frontmatter + Markdown body parser |
| Skill loader | ✅ Built | Multi-dir scan, create/delete user skills |
| Skill index | ✅ Built + 5 tests | Keyword search, progressive disclosure prompt |
| Long-term memory | ✅ Built + 6 tests | SQLite storage, text search, project filtering |
| Compaction | ✅ Built + 10 tests | Threshold-based, protects recent/debug/open-tool-call messages |

## Phase 7: Persona ✅

| Component | Status | Notes |
|---|---|---|
| PersonaPolicy | ✅ Built + 7 tests | Full system prompt generation at 3 intensity levels |
| CognitivePolicy | ✅ Built + 2 tests | 4 hard rules that never turn off |
| BehavioralPolicy | ✅ Built + 2 tests | 9 traits with prompt fragments |
| Intensity control | ✅ Built + 4 tests | Off/Subtle/Full with serde serde roundtrip |
| Original writing | ✅ Verified | All prompt text is original, no copyrighted material |

## Phase 8: UI / UX ✅

| Component | Status | Notes |
|---|---|---|
| Custom titlebar | ✅ Built | Transparent, drag-region |
| Sidebar | ✅ Built | Session list, create/delete/select |
| Chat view | ✅ Built | Message bubbles, composer, loading state |
| Design tokens | ✅ Built | Tailwind v4, indigo accent, dark-first |
| Tauri commands | ✅ Built + IPC | 12 commands wired |

## Phase 9: Hardening ✅

| Component | Status | Notes |
|---|---|---|
| Unit tests | ✅ 120 tests passing | Each crate independently tested |
| TypeScript check | ✅ Clean | Zero errors |
| Cargo check | ✅ Clean | Only warnings (unused fields) |
| Git integration | ✅ Built | git2-rs: status, diff, log, blame, stage, commit, branch |
| GitHub device flow | ✅ Built | OAuth device flow implementation |
| Auto-updater config | ✅ Configured | Tauri updater plugin with GitHub Releases |

## Not Yet Built

These are genuinely out of scope for the current milestone and will be implemented in subsequent work:

- **Streaming chat responses** — current implementation is non-streaming (sable-app/src/commands.rs uses `stream: false`). SSE streaming and the `OrchestratorEvent` event bus are architecturally ready but not yet wired to the Tauri frontend event system.
- **sable-lsp MCP server** — diagnostics bridge is designed but the server binary isn't implemented.
- **sable-git MCP server** — git engine is built as a Rust crate; the standalone MCP server process wrapping it is not yet created.
- **Formatter/linter auto-run** — the language toolchain registry and auto-verification after file writes.
- **Two-pass verification** for risky fixes.
- **Reasoning Tape UI** — the collapsible side-rail for reasoning/thinking blocks.
- **Live stats panel** — token/cost/context-window usage display.
- **GitHub PR creation** from chat UI.
- **Auto-updater** — configured but no release channel to point at yet.
- **Bundled MCP server binaries** — the Node.js MCP servers are source-only; need packaging as standalone executables or bundling with the Tauri binary.

## Build & Test

```bash
# Rust
cd src-tauri && cargo check    # all crates compile
cargo test                     # 120 tests pass

# TypeScript
pnpm install && npx tsc --noEmit  # zero errors
```