```
                    ██╗      █████╗ ███████╗██╗   ██╗ █████╗  ██████╗ ███████╗███╗   ██╗████████╗
                    ██║     ██╔══██╗╚══███╔╝╚██╗ ██╔╝██╔══██╗██╔════╝ ██╔════╝████╗  ██║╚══██╔══╝
                    ██║     ███████║  ███╔╝  ╚████╔╝ ███████║██║  ███╗█████╗  ██╔██╗ ██║   ██║
                    ██║     ██╔══██║ ███╔╝    ╚██╔╝  ██╔══██║██║   ██║██╔══╝  ██║╚██╗██║   ██║
                    ███████╗██║  ██║███████╗   ██║   ██║  ██║╚██████╔╝███████╗██║ ╚████║   ██║
                    ╚══════╝╚═╝  ╚═╝╚══════╝   ╚═╝   ╚═╝  ╚═╝ ╚═════╝ ╚══════╝╚═╝  ╚═══╝   ╚═╝
```

A [lazygit](https://github.com/jesseduffield/lazygit)-style TUI for managing AI coding agent sessions. Think htop, but for your AI agents.

You're running Claude Code in one tmux pane, Codex CLI in another, maybe a third agent working on tests — LazyAgent discovers them all, shows their live status, and lets you jump between them without losing your flow.

## Why

If you use AI coding agents seriously, you end up with multiple sessions scattered across tmux windows. Checking which one needs input, which one is thinking, which one errored out — it's tedious. LazyAgent solves this by giving you a single dashboard to monitor and manage all of them.

## Features

- **Auto-discovery** — Finds running agent sessions across all tmux panes, no setup needed
- **Live status** — Real-time detection: Thinking, Waiting, NeedsInput, Idle, Error
- **Spawn & attach** — Start new sessions or jump into existing ones with a keystroke
- **Multi-provider** — Claude Code and Codex CLI supported, extensible via trait-based plugin system
- **Session grouping** — Flat list, group by git root, or custom groups
- **Search** — Filter sessions by name or project
- **Configurable** — Keybindings, layout, theme, timing — all via `~/.config/lazyagent/config.toml`

## Requirements

- Rust (for building)
- tmux (sessions run inside tmux)
- One or more supported AI agents installed:
  - [Claude Code](https://docs.anthropic.com/en/docs/claude-code)
  - [Codex CLI](https://github.com/openai/codex)

## Install

### Shell (macOS / Linux)

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/tim101010101/lazyagent/releases/latest/download/lazyagent-installer.sh | sh
```

### Cargo

```sh
cargo install --git https://github.com/tim101010101/lazyagent
```

### Build from source

```sh
git clone https://github.com/tim101010101/lazyagent.git
cd lazyagent
cargo install --path .
```

## Usage

Make sure you're inside a tmux session, then:

```sh
lazyagent
```

LazyAgent will scan all tmux panes for running agent processes and display them in a navigable list with live status indicators.

## Keybindings

| Key | Action |
|---|---|
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `g` / `G` | Jump to top / bottom |
| `Enter` | Attach to session |
| `n` | Spawn new session |
| `d` → `y` | Kill session (with confirmation) |
| `i` | Passthrough mode (`Esc Esc` to exit) |
| `l` / `h` | Show / hide detail panel |
| `Tab` | Cycle grouping mode |
| `/` | Search |
| `r` | Refresh |
| `?` | Help overlay |
| `q` | Quit |

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for architecture details, how to add providers, configuration, and development setup.

## Roadmap

- [ ] Desktop notifications on NeedsInput / Error state changes
- [ ] Session rename with custom labels
- [ ] Remote tmux host support via SSH
- [ ] Session metrics (uptime, token usage, cost tracking)

## License

MIT
