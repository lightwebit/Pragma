# Changelog

All notable changes to Pragma are documented here.  
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).  
Pragma uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [0.1.0] — 2026-05-02

First complete release.

### Added

**Core protocol**
- Pragma protocol: structured phases ANALYSIS / QUESTIONS / PLAN / STEP_COMPLETE / REPORT / TESTS / CLOSING
- `PragmaEmitState` — per-session state machine with streaming deduplication and implicit-analysis fallback
- Protocol injected via `--append-system-prompt` + `<pragma>` prefix in user message for reliable compliance
- Typed atom stream: `FileTouch`, `Diff`, `ToolUse`, `Error`, `AgentNote`, `UserReply`, `PragmaEvent`

**UI — three-panel layout**
- Left panel: session list with search (FTS5), save/delete/duplicate, "running…" indicator
- Center panel: `AnalysisCard`, `QuestionsCard`, `PlanCard`, `TestsCard`, `ReportCard`, `ClosingCard` — collapsible with CSS grid animation
- Right panel: raw atom stream (`AtomStream.vue`) with per-atom expand/collapse
- Resizable panels (drag handle), persistent widths via localStorage
- Dark / light theme toggle, persisted across sessions
- Focus mode — filters noise, shows only structured pragma events
- Token counter live in header during active session

**Sessions**
- Multi-turn sessions with `--resume` / `--session-id`
- Session context card shown on resume
- New session on `+` button or `ClosingCard` confirm
- Guard prompt when switching away from a running session
- SQLite storage (WAL + FTS5) in `~/.pragma/pragma.db`
- Full-text search across session history
- Save / delete / duplicate from session list
- Session title auto-set from first prompt; editable

**Profiles**
- Multiple profiles with independent binary path, config dir, language preference
- Default profile (★) selector
- Model selection per-session: Sonnet (default) or Opus

**File attachments**
- File picker + drag-and-drop overlay
- Files copied to `{workingDir}/pragmadocs/` and passed via `--add-dir`
- Attachment chips in composer; red chip if file missing at resume
- Attachments persisted in DB per session

**Working directory**
- Directory picker with auto-open on first attach
- Trust store (`~/.pragma/trusted_dirs.json`) — explicit approval before use
- `pragmadocs/` subdirectory created automatically

**Export**
- Export session as JSON (UI button + CLI `pragma export`)
- Export session as Markdown (UI button, writes to configured export directory)
- Export directory configurable in settings (default: `~/Downloads`)
- Toast notification on export success

**Usage**
- `/usage` command via PTY — reads Claude Code's interactive usage screen
- Structured JSON output with three sections: current session, current week, extra usage
- Progress bars with warning/danger thresholds in modal UI
- Auto-confirm trust prompt; 20 s hard timeout

**Settings**
- Settings panel (Ctrl+S): binary path, config dir, language, export dir, profile management
- Auto-detect `claude` binary on startup
- Prompt files stored externally in `~/.pragma/prompts/<label>.txt`
- `diffAlwaysOpen` toggle

**Notifications**
- Desktop notifications via `tauri-plugin-notification`
- Permission requested lazily on first session completion (not at startup)

**Keyboard shortcuts**
- Ctrl+Enter — submit prompt
- Ctrl+S — open settings
- Esc — close composer / modal

### Security

- Binary path validated before spawn (rejects empty, `..` traversal, non-existent absolute paths)
- TOCTOU pre-check removed from attachment copy — `canonicalize()` used instead
- Session titles and attachment filenames sanitized (control chars stripped, 200-char limit)
- Only `http://` and `https://` URIs allowed in `open_url`
- Raw error details emitted only to `session:debug` channel; user-facing errors are structured codes
- DB file permissions restricted to owner on first open: `icacls` (Windows) / `chmod 700` (Unix)
- Working-directory trust store — `--dangerously-skip-permissions` gated on explicit user approval

### Infrastructure

- Tauri v2 desktop shell (Windows primary target; macOS / Linux path prepared)
- Single-instance guard via `tauri-plugin-single-instance`
- SQLite schema versioning + migration runner (`schema_version` table)
- `pragma-parser` and `pragma-cli` as independent Rust crates in a Cargo workspace
- `pragma-parser/src/lib.rs` split into `protocol.rs`, `process.rs`, `files.rs`, `usage.rs`, `trust.rs`
- `PRAGMA_CONSTANTS.ts` — all frontend magic numbers extracted
- Test coverage: 11 Vitest (atom filter), 6 Rust tauri (sanitize + copy), 4 Rust parser (storage round-trip)
- CI: GitHub Actions — frontend (type-check + Vitest + Vite build) + Rust (fmt + clippy + test, full workspace)
- MIT license, README, CONTRIBUTING

[0.1.0]: https://github.com/lightwebit/mirror.pragma.app/releases/tag/v0.1.0
