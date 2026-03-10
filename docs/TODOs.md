# TODOs

## Desktop Notifications for Session State Changes

- **Status**: 🔴 Pending
- **Priority**: 🟡 Medium
- **Created**: 2026-03-09
- **Tags**: feature, ux, notifications

### Background

When managing multiple agent sessions, users need to know when a session requires attention without constantly checking the TUI. Desktop notifications on state transitions (NeedsInput/Error) enable "fire and forget" workflow.

### Action Items

- [ ] Detect platform (macOS/Linux) and choose notification backend (osascript/notify-send)
- [ ] Add notification trigger in status detection logic when session transitions to NeedsInput or Error
- [ ] Make notifications opt-in via config flag (default: enabled)
- [ ] Include session name and provider in notification body

---

## Session Rename with Custom Labels

- **Status**: 🔴 Pending
- **Priority**: 🟢 Low
- **Created**: 2026-03-09
- **Tags**: feature, ux

### Background

Auto-generated session names like `la/claude/proj-a` are functional but not always meaningful. Users should be able to add custom labels for easier identification in the session list.

### Action Items

- [ ] Add `r` key binding to trigger rename mode
- [ ] Store custom labels in session metadata (separate from tmux session name)
- [ ] Display custom label in sidebar if set, fall back to auto-generated name
- [ ] Persist labels across TUI restarts (consider config file or tmux user-options)

---

## Remote Tmux Host Support via SSH

- **Status**: 🔴 Pending
- **Priority**: 🟡 Medium
- **Created**: 2026-03-09
- **Tags**: feature, remote, ssh

### Background

Design doc (AC-7/AC-8) specifies remote tmux discovery and management. Users working across dev servers need unified view of local + remote agent sessions.

### Action Items

- [ ] Implement `src/remote/mod.rs` with SSH wrapper for tmux commands
- [ ] Add remote host config loading from `~/.config/lazyagent/config.toml`
- [ ] Use SSH ControlMaster for connection reuse
- [ ] Update SessionManager to poll remote hosts (3s interval as per design)
- [ ] Display remote sessions with source marker in sidebar
- [ ] Support attach/kill for remote sessions

---

## Session Metrics Tracking

- **Status**: 🔴 Pending
- **Priority**: 🟡 Medium
- **Created**: 2026-03-09
- **Tags**: feature, metrics, analytics

### Background

Users managing long-running agent sessions need visibility into token consumption and cost. Provider JSONL files already contain usage data — parsing them gives us cost tracking for free.

### Action Items

- [ ] Track session uptime (already have `started_at`, compute duration on render)
- [ ] Parse provider JSONL files for token usage (Claude: `~/.claude/projects/` JSONL, Codex: rollout JSONL)
- [ ] Estimate cost based on token usage and provider pricing (configurable rates)
- [ ] Display cumulative tokens / estimated cost in detail panel
- [ ] Consider persisting metrics to disk for historical analysis

---

## Multi-select Batch Operations

- **Status**: 🔴 Pending
- **Priority**: 🟡 Medium
- **Created**: 2026-03-10
- **Tags**: feature, ux

### Background

Managing 10+ stale sessions one-by-one is painful. Multi-select with batch kill/restart significantly improves session cleanup workflow.

### Action Items

- [ ] Add `Space` keybinding to toggle selection on current session
- [ ] Visual indicator (checkbox/highlight) for selected sessions in sidebar
- [ ] `D` (shift-d) to batch kill all selected sessions (with confirmation)
- [ ] Clear selection on action completion or `Esc`
- [ ] Consider batch restart for selected sessions

---

## Additional Provider Support

- **Status**: 🔴 Pending
- **Priority**: 🟡 Medium
- **Created**: 2026-03-10
- **Tags**: feature, providers

### Background

Only Claude and Codex are supported. Other CLI-based AI agents (Aider, Goose, etc.) have growing user bases. The provider trait is already designed for extensibility — adding new providers is low-cost.

### Action Items

- [ ] Implement AiderProvider (`src/provider/aider.rs`) — process matching + status detection
- [ ] Implement GooseProvider (`src/provider/goose.rs`)
- [ ] Survey other CLI agents with tmux-compatible workflows
- [ ] Document provider authoring guide for community contributions
- [ ] Consider dynamic provider loading via config

---

## Session Sorting

- **Status**: 🔴 Pending
- **Priority**: 🔴 High
- **Created**: 2026-03-10
- **Tags**: feature, ux

### Background

Currently sessions are grouped but not sorted within groups. Sorting by status (NeedsInput pinned to top), last activity, or name helps users quickly find sessions that need attention. NeedsInput-first sorting is the highest-value improvement here.

### Action Items

- [ ] Add sort mode cycling via keybinding (e.g. `s`)
- [ ] Sort options: by status priority (NeedsInput > Error > Thinking > Waiting > Idle > Unknown), by name, by last activity
- [ ] Default sort: status priority (NeedsInput always on top)
- [ ] Persist sort preference in config
- [ ] Display current sort mode in help bar

---

## Keybinding Modifier Support

- **Status**: 🔴 Pending
- **Priority**: 🟢 Low
- **Created**: 2026-03-10
- **Tags**: feature, ux, keybindings

### Background

Current keybinding system only supports single-character keys. As features grow, key space becomes limited. Supporting Ctrl/Alt modifiers expands available bindings and aligns with terminal app conventions.

### Action Items

- [ ] Extend keybinding parser to handle `C-x` (Ctrl) and `A-x` (Alt) notation
- [ ] Update crossterm event matching to support modifier keys
- [ ] Migrate config format to support modifier syntax in `config.toml`
- [ ] Add default modifier bindings for new features (keep existing single-char bindings stable)
- [ ] Update help overlay to display modifier key combos
