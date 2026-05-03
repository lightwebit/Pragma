# Contributing to Pragma

Pragma is a small personal project. At the moment it is developed by one person with Claude as an AI collaborator. External contributions are welcome but the project moves at a deliberate pace and PRs may take time to review.

## Prerequisites

| Tool | Version | Install |
|---|---|---|
| Rust | stable | https://rustup.rs |
| Node.js | 22 | https://nodejs.org |
| Tauri CLI | v2 | `cargo install tauri-cli --version "^2"` |
| Claude Code | latest | https://claude.ai/code |

Claude Code must be installed and authenticated — Pragma spawns the `claude` binary at runtime.

On Windows, [WebView2](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) is required (included in Windows 11).

## Development setup

```sh
git clone https://github.com/lightwebit/pragma
cd pragma
npm install
npm run tauri:dev
```

`tauri:dev` starts Vite with hot-reload and the Tauri dev window simultaneously.

## Project layout

```
pragma-parser/      Rust crate — parses agent stdout into typed Atom stream
pragma-cli/         Rust crate — CLI wrapper binary
src-tauri/          Tauri backend — commands, IPC, SQLite storage
src-tauri/src/
  lib.rs            Tauri command handlers + app entry point
  protocol.rs       Pragma protocol state machine (PragmaEmitState)
  process.rs        Claude process spawning + stream_claude
  files.rs          File utilities (sanitize, copy_to_pragmadocs)
  usage.rs          PTY-based /usage command parsing
  trust.rs          Working-directory trust store
src/                Vue 3 frontend — App.vue, components, Pinia store
```

## Running tests

```sh
# Frontend (Vitest)
npm test

# Rust — parser crate
cargo test -p pragma-parser

# Rust — Tauri backend
cargo test -p pragma-tauri
```

All three test suites must pass before opening a PR.

## Code conventions

- **Rust**: `cargo clippy` with no new warnings; `cargo fmt` applied.
- **TypeScript/Vue**: no new TypeScript errors (`npx tsc --noEmit`); no magic numbers (add constants to `src/PRAGMA_CONSTANTS.ts`).
- No new comments that just describe what the code does — only add a comment when the *why* is non-obvious.
- Keep the module split: logic in the appropriate `src-tauri/src/*.rs` module, thin wrappers in `lib.rs`.

## Opening an issue

Before opening a PR, open an issue describing the problem or feature. Small fixes (typos, one-line bugs) can skip this step.

## License

By contributing you agree that your work will be released under the [MIT License](LICENSE).
