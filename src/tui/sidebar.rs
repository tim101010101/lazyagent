use ratatui::{
    style::Color,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState},
    Frame,
};

use crate::app::SidebarItem;
use crate::config::SidebarConfig;
use crate::protocol::{AgentSession, AgentStatus, SessionSource};
use crate::tui::theme::Theme;

pub fn render(
    frame: &mut Frame,
    area: Rect,
    items: &[SidebarItem],
    sessions: &[AgentSession],
    selected: usize,
    focused: bool,
    tick: u64,
    theme: &Theme,
    sidebar_config: &SidebarConfig,
) {
    let border_style = if focused {
        theme.border_focused
    } else {
        theme.border_unfocused
    };

    let title = " Sessions ";

    let block = Block::default()
        .title(title)
        .title_style(theme.title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style);

    let inner = block.inner(area);

    let mut session_row_idx: usize = 0;
    let list_items: Vec<ListItem> = items
        .iter()
        .enumerate()
        .map(|(item_idx, item)| match item {
            SidebarItem::SourceHeader(name) => {
                let marker = if name == "local" {
                    &sidebar_config.local_marker
                } else {
                    &sidebar_config.remote_marker
                };
                ListItem::new(Line::from(Span::styled(
                    format!(" {} {} ", marker, name),
                    theme.source_header,
                )))
            }
            SidebarItem::GroupHeader(name) => {
                let inner_w = area.width.saturating_sub(2) as usize;
                let display = shorten_path(name);
                let fill_len = inner_w.saturating_sub(display.len() + 4);
                let fill = "─".repeat(fill_len);
                let text = format!(" ─ {} {}", display, fill);
                ListItem::new(Line::from(Span::styled(text, theme.project_header)))
            }
            SidebarItem::Session(idx) => {
                let is_selected = item_idx == selected;
                let row_idx = session_row_idx;
                session_row_idx += 1;
                if let Some(session) = sessions.get(*idx) {
                    let mut item = render_session_item(session, area.width, tick, theme, is_selected);
                    if row_idx % 2 == 0 {
                        item = item.style(ratatui::style::Style::default().bg(Color::Rgb(46, 52, 64)));
                    }
                    item
                } else {
                    ListItem::new(Line::from(""))
                }
            }
        })
        .collect();

    let mut state = ListState::default();
    state.select(Some(selected));

    let list = List::new(list_items).highlight_style(theme.selected);

    frame.render_widget(block, area);
    frame.render_stateful_widget(list, inner, &mut state);
}

fn render_session_item(
    session: &AgentSession,
    width: u16,
    tick: u64,
    theme: &Theme,
    is_selected: bool,
) -> ListItem<'static> {
    let (icon, icon_style) = status_icon(&session.status, tick, theme);

    let time_str = session
        .started_at
        .map(format_relative_time)
        .unwrap_or_default();

    let source_marker = match &session.source {
        SessionSource::Remote { .. } => " [R]",
        SessionSource::Local => "",
    };

    let cwd_short = shorten_path(&session.cwd.to_string_lossy());

    // Format: "▌ icon provider  cwd  time [R]"
    let fixed_len = 4 + session.provider.len() + 2 + time_str.len() + source_marker.len() + 2;
    let cwd_max = (width as usize).saturating_sub(fixed_len);
    let cwd_display = truncate_str(&cwd_short, cwd_max);

    let bar = if is_selected {
        Span::styled("▌ ", theme.selected_bar)
    } else {
        Span::styled("  ", theme.normal)
    };

    ListItem::new(Line::from(vec![
        bar,
        Span::styled(icon.to_string(), icon_style),
        Span::styled(format!(" {} ", session.provider), theme.normal),
        Span::styled(cwd_display, theme.label),
        Span::styled(format!("  {}", time_str), theme.label),
        Span::styled(source_marker.to_string(), theme.label),
    ]))
}

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

fn status_icon(
    status: &AgentStatus,
    tick: u64,
    theme: &Theme,
) -> (&'static str, ratatui::style::Style) {
    match status {
        AgentStatus::Waiting | AgentStatus::Idle => ("●", theme.status_active),
        AgentStatus::NeedsInput => ("◆", theme.status_needs_input),
        AgentStatus::Thinking => {
            let frame = (tick as usize) % SPINNER_FRAMES.len();
            (SPINNER_FRAMES[frame], theme.status_thinking)
        }
        AgentStatus::Error => ("✖", theme.status_error),
        AgentStatus::Unknown => ("?", theme.status_unknown),
    }
}

pub fn shorten_path(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if parts.len() <= 2 {
        return path.to_string();
    }
    format!("~/{}", parts[parts.len() - 2..].join("/"))
}

pub fn truncate_str(s: &str, max_len: usize) -> String {
    let char_count: usize = s.chars().count();
    if char_count <= max_len {
        s.to_string()
    } else if max_len > 3 {
        let truncated: String = s.chars().take(max_len - 3).collect();
        format!("{}...", truncated)
    } else {
        s.chars().take(max_len).collect()
    }
}

pub fn format_relative_time(time: std::time::SystemTime) -> String {
    let elapsed = time.elapsed().unwrap_or_default();
    let secs = elapsed.as_secs();

    if secs < 60 {
        "just now".into()
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else if secs < 86400 * 30 {
        format!("{}d ago", secs / 86400)
    } else {
        format!("{}mo ago", secs / (86400 * 30))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shorten_path() {
        assert_eq!(shorten_path("/home/user/Code/app"), "~/Code/app");
        assert_eq!(shorten_path("/app"), "/app");
        assert_eq!(shorten_path("~/short"), "~/short");
    }

    #[test]
    fn test_truncate_str() {
        assert_eq!(truncate_str("hello", 10), "hello");
        assert_eq!(truncate_str("hello world foo", 10), "hello w...");
        assert_eq!(truncate_str("修复图片加载失败显示问题", 6), "修复图...");
    }
}
