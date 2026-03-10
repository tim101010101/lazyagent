# LazyAgent MVP: tmux-based Active Agent Session Manager — Design

Date: 2026-03-09
Status: ready

## Summary

Pivot LazyAgent from a historical session viewer/resume launcher to an active agent session manager built on tmux. The TUI discovers, spawns, attaches to, and kills live AI coding agent sessions across local and remote tmux servers. tmux server acts as the daemon — no custom PTY management or socket protocol needed.

## Design

### Core Positioning

LazyAgent is an intelligent management layer on top of tmux, specifically for AI coding agent sessions. It is NOT a general tmux manager — only agent sessions are visible.

Differentiators vs competitors (Claude Squad, Agent Deck, Agent of Empires):
- Discovers externally-started agent sessions in tmux (not just self-spawned)
- Unified view of local + remote tmux servers
- Agent-aware status detection (thinking/waiting/idle/error)

### Session Model

```rust
enum SessionKind {
    /// Spawned by lazyagent, fully controlled
    Managed,
    /// Discovered in tmux by process/output matching
    Discovered,
}

struct AgentSession {
    kind: SessionKind,
    tmux_session: String,       // tmux session name
    tmux_pane: String,          // pane id
    provider: String,           // "claude", "aider", "codex"
    cwd: PathBuf,
    status: AgentStatus,        // Thinking, Waiting, Idle, Error, Unknown
    started_at: Option<SystemTime>,
    source: SessionSource,
}

enum SessionSource {
    Local,
    Remote { host: String },
}
```

### Session Naming Convention

Managed sessions use name prefix `la/` with structured naming:
```
la/claude/proj-a
la/aider/my-api
```

This enables O(1) distinction: `tmux list-sessions -f '#{m:la/*,#{session_name}}'` filters managed sessions in a single call. External sessions are discovered separately by scanning all panes.

### Session Discovery

#### Local Discovery (single call + selective capture)

```
Step 1: tmux list-panes -a -F '#{session_name} #{pane_id} #{pane_pid} #{pane_current_command} #{pane_current_path}'
  → Get all panes with metadata in ONE call

Step 2: Filter panes where pane_current_command matches known agents
  → claude, aider, codex, etc.

Step 3: For matched panes, tmux capture-pane -p -t <pane_id>
  → Only capture panes that need status detection
```

#### Remote Discovery

Same commands over SSH with ControlMaster:
```
ssh -o ControlMaster=auto host tmux list-panes -a -F '...'
ssh host tmux capture-pane -p -t <pane_id>
```

#### Remote Host Configuration

```toml
# ~/.config/lazyagent/config.toml
[[remote]]
host = "dev-server"
# ssh_options = "-p 2222"  # optional

[[remote]]
host = "gpu-box"
```

### Session Lifecycle

#### Spawn (Managed)

```
1. Provider.exec_plan(cwd) → program, args, env
2. tmux new-session -d -s "la/<provider>/<project>" -c <cwd> -- <program> <args>
3. Session appears in TUI on next poll
```

#### Attach

Two modes:
- **Fullscreen**: suspend TUI → `tmux attach -t <session>` → exit returns to TUI
- **Embedded panel**: `capture-pane` output rendered in ratatui panel (P1)

#### Kill

```
tmux kill-session -t <session>
```

With confirmation prompt in TUI.

### Agent Status Detection

Each Provider defines detection patterns applied to `capture-pane` output (last N lines):

```rust
trait Provider {
    fn manifest(&self) -> ProviderManifest;
    fn exec_plan(&self, cwd: &Path) -> ExecPlan;
    fn detect_status(&self, pane_output: &str) -> AgentStatus;
    fn match_process(&self, process_name: &str) -> bool;
}
```

Example patterns:
| Provider | Waiting | Thinking | Error |
|----------|---------|----------|-------|
| claude | `❯` or `>` prompt at end | spinner chars, `⠋⠙⠹...` | `Error:`, red ANSI codes |
| aider | `>` prompt at end | `Thinking...` | `Error`, traceback |

Status is coarse-grained. `Unknown` is acceptable — no false positives over false negatives.

### Polling Strategy

| Source | Interval | Method |
|--------|----------|--------|
| Local list | 1s | `tmux list-panes -a -F` (single call) |
| Local status | 1s | `capture-pane` per agent pane |
| Remote list | 3s | SSH + `tmux list-panes -a -F` |
| Remote status | 3s | SSH + `capture-pane` per agent pane |

Optimization: skip `capture-pane` for panes whose `pane_pid` hasn't changed since last poll.

### TUI Layout

```
┌─ Sessions ─────────────────────────────────────────┐
│ ● claude  ~/proj-a          thinking...    2h ago  │
│ ● claude  ~/proj-b          waiting input  30m ago │
│ ● aider   ~/proj-c          idle           5m ago  │
│ ◆ dev:claude  ~/api         editing file   1h ago  │
│ ◆ dev:claude  ~/web         running tests  10m ago │
└────────────────────────────────────────────────────┘
┌─ Detail ───────────────────────────────────────────┐
│ Provider: claude    Status: thinking               │
│ CWD: ~/Code/proj-a                                 │
│ Session: la/claude/proj-a   PID: 12345             │
│ Source: local       Uptime: 2h 15m                 │
└────────────────────────────────────────────────────┘
┌─ Keys ─────────────────────────────────────────────┐
│ enter:attach  n:new  k:kill  /:search  ?:help      │
└────────────────────────────────────────────────────┘

● = local managed/discovered
◆ = remote
```

### Key Bindings

| Key | Action |
|-----|--------|
| `j/k` | Navigate sessions |
| `Enter` | Attach fullscreen |
| `n` | New session (select provider + cwd) |
| `k` (with modifier) or `d` | Kill session (with confirmation) |
| `/` | Search/filter |
| `r` | Refresh |
| `g/G` | Jump top/bottom |
| `q` | Quit TUI (sessions keep running) |
| `l` | Toggle detail panel |

## Changes

| File | Action | Description |
|------|--------|-------------|
| `src/protocol/mod.rs` | modify | Redefine types: AgentSession, SessionKind, SessionSource, AgentStatus |
| `src/protocol/provider.rs` | modify | Update Provider trait: add `detect_status()`, `match_process()`, remove `list_sessions()`, `session_detail()` |
| `src/provider/claude.rs` | modify | ClaudeProvider: implement `detect_status` patterns, `match_process` for claude binary |
| `src/tmux/mod.rs` | modify | Rewrite TmuxController: discovery (list-panes -a), spawn (new-session -d), attach, kill. Remove split-pane logic |
| `src/remote/mod.rs` | new | RemoteTmux: SSH wrapper for remote tmux commands, host config loading |
| `src/session/mod.rs` | new | SessionManager: unifies local + remote discovery, manages poll loop, deduplication |
| `src/app.rs` | modify | Replace sidebar_items with AgentSession list, update key handling for new actions |
| `src/tui/sidebar.rs` | modify | Render AgentSession list with status indicators and source markers |
| `src/tui/detail.rs` | modify | Render AgentSession detail (provider, status, cwd, source, uptime) |
| `src/config.rs` | new | Config loading: remote hosts, polling intervals, provider settings |
| `src/main.rs` | modify | Update event loop: poll-based session refresh, remove old JSONL loading |

## Acceptance Criteria

- [ ] AC-1: LazyAgent discovers running claude/aider processes in local tmux panes and displays them in TUI
- [ ] AC-2: User can spawn a new agent session from TUI; session runs in a detached tmux session with `la/` prefix
- [ ] AC-3: User can fullscreen-attach to any listed session; exiting returns to TUI; session keeps running
- [ ] AC-4: User can kill a session from TUI with confirmation
- [ ] AC-5: Agent status (thinking/waiting/idle/error/unknown) is detected via capture-pane and displayed in session list
- [ ] AC-6: Quitting LazyAgent TUI does not terminate any agent sessions
- [ ] AC-7: Remote tmux sessions are discovered and displayed when remote hosts are configured
- [ ] AC-8: User can attach to and kill remote sessions with same UX as local
- [ ] AC-9: Polling uses batch `list-panes -a -F` (max 1 call for list per source per cycle)
- [ ] AC-10: Only agent sessions are shown — non-agent tmux sessions are excluded

## Unresolved Questions

None
