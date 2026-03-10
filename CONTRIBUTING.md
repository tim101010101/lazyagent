# Contributing to LazyAgent

## Build & Test

```sh
cargo build
cargo test        # 24 tests: e2e, provider, sidebar, tmux, rendering
cargo run         # requires tmux
```

## How It Works

LazyAgent polls tmux every 2 seconds:

1. `tmux list-panes -a` to enumerate all panes
2. Each pane's process tree is matched against registered providers
3. Matched sessions get their status resolved via provider-specific strategies:
   - **Claude Code**: reads JSONL session history + pane output text matching
   - **Codex CLI**: queries SQLite state DB + pane output text matching
4. Results are rendered in a ratatui-powered TUI

Sessions you spawn through LazyAgent are named `la/<provider>/<dir>` for easy identification. Externally-started sessions are discovered and shown alongside them.

## Architecture

```
src/
  main.rs          — entry, event loop, attach/spawn dispatch
  app.rs           — App state, key handling, session navigation, search
  event.rs         — crossterm event polling
  session.rs       — SessionManager: poll/spawn/attach/kill via TmuxController
  protocol/
    mod.rs         — types: AgentSession, AgentStatus, SessionKind, SessionSource, ExecPlan
    provider.rs    — Provider trait: detect_status, match_process, exec_plan
    status.rs      — StatusResolver framework
  provider/
    mod.rs         — provider registry
    claude.rs      — ClaudeProvider: JSONL history + pane output status detection
    codex.rs       — CodexProvider: SQLite state DB + pane output status detection
  tmux/
    mod.rs         — TmuxController: discover_sessions, spawn_session, attach/kill
  tui/
    mod.rs         — terminal init/restore
    layout.rs      — 2-col or 3-col horizontal layout
    sidebar.rs     — session list with status icons, grouping
    detail.rs      — detail panel: KV display of session info
    help_overlay.rs — help overlay with keybindings
    theme.rs       — color/style constants
  config/
    mod.rs         — config loading from ~/.config/lazyagent/config.toml
    keys.rs        — customizable keybindings
    layout.rs      — layout configuration
    theme.rs       — theme customization
    timing.rs      — timing parameters
```

## Adding a Provider

Providers implement the `Provider` trait:

```rust
pub trait Provider: Send + Sync {
    fn manifest(&self) -> ProviderManifest;
    fn match_process(&self, cmdline: &str) -> bool;
    fn exec_plan(&self, cwd: &str) -> ExecPlan;
    fn resolvers(&self) -> Vec<Box<dyn StatusResolver>>;
}
```

1. Create `src/provider/your_agent.rs`
2. Implement the trait
3. Register it in `src/provider/mod.rs`

LazyAgent will automatically discover sessions matching your provider.

## Configuration

```toml
# ~/.config/lazyagent/config.toml

[timing]
refresh_interval_ms = 2000

[layout]
detail_panel = true

[theme]
# customize colors
```

## Logging

Logs are written to `~/.local/state/lazyagent/`:

- `lazyagent.log.YYYY-MM-DD` — all logs
- `lazyagent-error.log.YYYY-MM-DD` — errors only

Override log level with `LAZYAGENT_LOG=debug lazyagent`.

## Code Conventions

- Rust 2021 edition, sync event loop (no async)
- Immutable ratatui frame rendering
- `anyhow::Result` for app-level errors
- Conventional commits: `feat/fix/refactor/docs/test/chore`
- Small functions (<50 lines), small files (<800 lines)
