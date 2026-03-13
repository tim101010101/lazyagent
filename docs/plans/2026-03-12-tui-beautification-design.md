# TUI Beautification — Design

Date: 2026-03-12
Status: ready

## Summary

Enhance LazyAgent's TUI visual design by adopting modern terminal UI patterns from lazygit, lazydocker, and htop. Add rounded borders, Nord color palette, header bar with session stats, sidebar enhancements (selected indicator, status footer, alternating rows), redesigned detail panel, and mode badges. All changes are purely visual — no functional behavior changes.

## Design

### Inspiration Sources

- **lazygit**: focused panel borders (bright cyan), compact info density, clear visual hierarchy
- **lazydocker**: status badges/pills with color coding, resource summary displays
- **htop**: top header bar with global stats, alternating row background tints

### Visual Changes

#### 1. Rounded Borders (All Panels)
Replace default square borders with `BorderType::Rounded` on all `Block` widgets:
- Sidebar panel
- Main panel
- Detail panel
- Help overlay modal

#### 2. Nord Color Palette (Theme Defaults)
Update `ThemeConfig::default()` in `src/tui/theme.rs` with Nord-inspired RGB colors:

| Style Slot | Current | New RGB | Nord Reference |
|------------|---------|---------|----------------|
| `title` | Cyan | `(136, 192, 208)` | Nord8 frost |
| `border_focused` | Cyan | `(136, 192, 208)` | Nord8 frost |
| `border_unfocused` | Dark Gray | `(67, 76, 94)` | Nord1 |
| `source_header` | Blue | `(129, 161, 193)` | Nord9 |
| `project_header` | Yellow | `(235, 203, 139)` | Nord13 |
| `status_thinking` | Yellow | `(235, 203, 139)` | Nord13 amber |
| `status_active` | Green | `(163, 190, 140)` | Nord14 sage |
| `status_needs_input` | Light Magenta | `(180, 142, 173)` | Nord15 purple |
| `status_error` | Red | `(191, 97, 106)` | Nord11 rose |
| `selected` bg | Dark Gray | `(59, 66, 82)` | Nord2 |

Add new style slot: `selected_bar: Style` — for the `▌` left indicator on selected sidebar items.

#### 3. Header Bar (New Component)
Add 1-line top bar above main layout showing:
```
  ⚡ LazyAgent   claude: ●3 ◆1 ⠏2 ✖0    12 sessions    03/12 14:23
```

Layout:
- **Left**: app name + version from `env!("CARGO_PKG_VERSION")`
- **Center**: per-provider status counts (icon + count, styled with status colors)
- **Right**: total session count + current time (HH:MM format)

Implementation:
- New file: `src/tui/header.rs` with `pub fn render(frame, area, sessions, tick)` function
- Update `src/tui/layout.rs`: add `Constraint::Length(1)` header row before existing vertical split
- Export from `src/tui/mod.rs`

#### 4. Sidebar Enhancements

**a) Selected Row Indicator**
Prepend `▌` (U+258C left half block) to selected item line, styled with `theme.selected_bar` color. Non-selected items get a space prefix for alignment.

**b) Group Separator Styling**
Replace plain `GroupHeader` text with styled horizontal rule:
```
 ─── ~/projects/myapp ──────────────────
```
Use box-drawing character `─` repeated to fill width, styled with `project_header` color.

**c) Panel Title with Counts**
Update sidebar block title to show selection position:
```
 Sessions [3 / 12]
```
Format: `[selected_index+1 / total_visible_sessions]`

**d) Alternating Row Tint**
Apply subtle background tint `Color::Rgb(46, 52, 64)` (Nord0+1 blend) to even-indexed session items only. Headers (SourceHeader, GroupHeader) remain untinted.

**e) Status Summary Footer**
Add 1-line footer inside sidebar block showing aggregated status counts:
```
  ● 3   ◆ 1   ⠏ 2   ✖ 0
```
Each count styled with its corresponding status color. Rendered as a `Paragraph` widget below the `List`.

Implementation: split sidebar `area` vertically into `[List area][footer 1 row]` using `Layout::vertical`.

#### 5. Detail Panel Redesign

Replace flat key-value list with status card layout:
```
╭─ Detail ──────────────────╮
│                            │
│   ⠏  Thinking              │  ← status row (3-space indent, large visual weight)
│                            │
│  Provider  claude          │  ← KV rows (label right-aligned, 10-char width)
│  CWD       ~/proj/foo      │
│  Uptime    5m 32s          │
│  Session   la/claude/foo   │
│  Source    Local           │
│                            │
╰────────────────────────────╯
```

Structure:
- **Status row**: icon + status text, styled with status color, no label prefix
- **Separator**: empty row after status
- **KV rows**: label in `theme.label` (right-aligned, fixed 10-char width), value in `theme.value`

#### 6. Help Bar Mode Badge

Add left-anchored mode badge before key hints:

| Mode | Badge | Style |
|------|-------|-------|
| Normal | `▐ NORMAL ▌` | Gray |
| Search | `▐ SEARCH ▌` | Yellow |
| Passthrough | `▐ PASS ▌` | Magenta Bold |
| Kill confirm | `▐ CONFIRM ▌` | Red |

Badge uses `▐` (U+2590 right half block) as visual bookends, echoing vim's mode indicator style.

#### 7. Main Panel Title Enhancement

Current format: `<provider> — <cwd>` or `PASSTHROUGH | <provider> — <cwd>`

New format: append status indicator + uptime:
```
 claude │ ~/projects/foo │ ● │ 5m
```

Use `│` (U+2502 box-drawing vertical) as separators. Status icon styled with status color.

## Changes

| File | Action | Description |
|------|--------|-------------|
| `src/tui/theme.rs` | modify | Update `ThemeConfig::default()` with Nord RGB colors, add `selected_bar` style slot |
| `src/tui/layout.rs` | modify | Add `Constraint::Length(1)` header row in vertical split |
| `src/tui/header.rs` | new | Header bar widget with app name, status counts, time |
| `src/tui/sidebar.rs` | modify | Add selected indicator, styled group separators, title counts, alternating rows, status footer |
| `src/tui/detail.rs` | modify | Redesign as status card + aligned KV grid |
| `src/tui/help.rs` | modify | Add left-anchored mode badge |
| `src/main.rs` | modify | Add rounded borders to main panel, enhance title with status + uptime |
| `src/tui/help_overlay.rs` | modify | Add rounded borders to help modal |
| `src/tui/mod.rs` | modify | Export new `header` module |

## Acceptance Criteria

- [ ] AC-1: All panel borders (sidebar, main, detail, help overlay) use `BorderType::Rounded`
- [ ] AC-2: Theme defaults use Nord RGB colors as specified in design table
- [ ] AC-3: Header bar renders at top with app name, per-provider status counts, total count, and current time
- [ ] AC-4: Selected sidebar item shows `▌` indicator, non-selected items show space
- [ ] AC-5: Sidebar group separators render as styled horizontal rules with `─` characters
- [ ] AC-6: Sidebar title shows `[N / M]` format with selection position and total count
- [ ] AC-7: Even-indexed session rows in sidebar have subtle background tint
- [ ] AC-8: Sidebar footer shows status counts with colored icons
- [ ] AC-9: Detail panel shows status as prominent card above KV grid with right-aligned labels
- [ ] AC-10: Help bar shows mode badge on left with appropriate color per mode
- [ ] AC-11: Main panel title includes status icon and uptime with `│` separators
- [ ] AC-12: All existing tests pass (`cargo test`)
- [ ] AC-13: Application compiles without warnings (`cargo build`)
- [ ] AC-14: Visual verification confirms all changes render correctly in terminal

## Unresolved Questions

None

