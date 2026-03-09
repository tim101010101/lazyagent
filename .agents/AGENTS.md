# LazyAgent

Lazygit-style TUI for managing live AI coding agent sessions via tmux. Rust + ratatui + crossterm.

## Architecture

```
src/
  main.rs          — entry, event loop, attach/spawn dispatch, render_main
  app.rs           — App state, key handling, session navigation, search, kill confirm
  event.rs         — crossterm event polling
  session.rs       — SessionManager: poll/spawn/attach/kill via TmuxController
  protocol/
    mod.rs         — types: AgentSession, AgentStatus, SessionKind, SessionSource, ExecPlan
    provider.rs    — Provider trait: detect_status, match_process, exec_plan
  provider/
    mod.rs         — provider registry
    claude.rs      — ClaudeProvider: process matching + pane output status detection
  tmux/
    mod.rs         — TmuxController: discover_sessions, spawn_session, attach/kill
  tui/
    mod.rs         — terminal init/restore
    layout.rs      — AppLayout: 2-col or 3-col horizontal
    sidebar.rs     — session list with status icons, source/project grouping
    detail.rs      — detail panel: KV display of session info
    help.rs        — help bar with context-aware hints
    theme.rs       — color/style constants incl status colors
```

## Key Patterns

- **Provider protocol**: trait-based plugin system — `detect_status()` for pane output, `match_process()` for discovery
- **Session discovery**: `tmux list-panes -a -F` → match process via providers → `capture-pane` for status
- **Managed sessions**: spawned with `la/<provider>/<dir>` naming convention
- **Discovered sessions**: externally-started agent sessions found in any tmux session
- **Attach flow**: suspend TUI → `tmux attach-session -t <name>` → restore TUI on exit
- **Auto-refresh**: sessions polled every 2s via tick counter
- **Kill confirmation**: `d` sets `confirm_kill`, `y/n` to confirm/cancel

## Code Conventions

- Rust 2021 edition, no async in main loop (sync event polling, 100ms tick)
- Immutable ratatui frame rendering, no stateful widgets
- CJK-aware text truncation
- All tests use `#[test]`, MockProvider with TestBackend — no external test framework
- Prefer `anyhow::Result` for app-level errors

## Build & Test

```sh
cargo build
cargo test        # 24 tests: e2e, provider, sidebar, tmux, rendering
cargo run         # requires tmux
```

## Logging

- tracing-based structured logging, default info level (override via `LAZYAGENT_LOG` env)
- Log dir: `~/.local/state/lazyagent/` (`dirs::state_dir()`)
- Files: `lazyagent.log.YYYY-MM-DD` (all), `lazyagent-error.log.YYYY-MM-DD` (errors only)
- Daily rotation, 7-day retention
- Use `/log-search` skill to search/filter logs

## Do Not

- Add async to the event loop without strong reason
- Break Provider trait backward compat without updating all providers
- Hardcode paths — use `dirs` crate for home, env vars for config
- Swallow errors silently — always surface via `app.error_message`
- Kill agent sessions on TUI quit — sessions must persist independently
