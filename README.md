# Vornix

An MCP-native, debugging-first AI coding harness. Desktop app built with Tauri v2 (Rust core) and React 19.

Vornix treats root-cause analysis as a first-class phase of the agent loop: mandatory hypothesis tracking, reproduce-before-fix discipline, and verification (formatters, linters, type-checkers, tests) enforced by the orchestrator — not by prompt suggestion. Every tool, including the bundled filesystem and shell access, is an MCP server speaking JSON-RPC 2.0, so "add a capability" and "connect an MCP server" are the same action.

## Highlights

- **Explicit agent state machine** — `IDLE → PLANNING → ACTING → OBSERVING → REFLECTING → VERIFYING`, with loop detection and iteration limits enforced in Rust
- **Real MCP everywhere** — bundled servers (`vornix-fs`, `vornix-shell`, `vornix-thinking`) run as child processes over stdio; user servers connect via stdio or HTTP
- **Three-tier permission gate** — read-only / reversible-write / destructive, classified and enforced in the Rust core, not the system prompt
- **Persona-driven voice** — original behavioral policy engine (Off / Subtle / Full); the voice changes, the rigor never does
- **OS keychain secrets** — provider keys never touch plaintext on disk (age-encrypted file fallback where no keyring exists)
- **Skills** — portable `SKILL.md` format with progressive disclosure; agent-authored skills gated behind user approval

## Status

See [docs/STATUS.md](docs/STATUS.md) for the honest ledger of what's implemented and verified vs. designed-but-not-yet-built. Architecture overview lives in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Development

```bash
pnpm install
cargo tauri dev        # from repo root; needs Rust, Node 22+, and webkit2gtk on Linux
```

```bash
cd src-tauri
cargo test -p vornix-core -p vornix-tools -p vornix-memory \
  -p vornix-persona -p vornix-skills -p vornix-secrets   # unit tests
cargo test --test vornix_e2e -- --nocapture              # live end-to-end (needs OpenRouter key in OS keychain)
```

The e2e test exercises the full stack for real: a free OpenRouter model drives the tool loop through an actual MCP child process and writes files to disk.

## License

MIT
