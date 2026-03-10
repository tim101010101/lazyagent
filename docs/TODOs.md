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
- **Priority**: 🟢 Low
- **Created**: 2026-03-09
- **Tags**: feature, metrics, analytics

### Background

Users managing long-running agent sessions would benefit from visibility into session duration, resource usage, and cost. Helps with budgeting and identifying inefficient workflows.

### Action Items

- [ ] Track session uptime (already have `started_at`, compute duration on render)
- [ ] Add optional token usage tracking if provider exposes API (Claude API usage endpoint)
- [ ] Estimate cost based on token usage and provider pricing
- [ ] Display metrics in detail panel
- [ ] Consider persisting metrics to disk for historical analysis
