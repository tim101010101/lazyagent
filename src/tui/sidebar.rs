use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};

use crate::app::{GroupingMode, SidebarItem};
use crate::protocol::{AgentSession, AgentStatus, SessionSource};
use crate::tui::theme::Theme;

pub fn render(
    frame: &mut Frame,
    area: Rect,
    items: &[SidebarItem],
    sessions: &[AgentSession],
    selected: usize,
    focused: bool,
    grouping_mode: &GroupingMode,
) {
    let border_style = if focused {
        Theme::border_focused()
    } else {
        Theme::border_unfocused()
    };

    let title = format!(" Sessions [{}] ", grouping_mode.label());

    let block = Block::default()
        .title(title)
        .title_style(Theme::title())
        .borders(Borders::ALL)
        .border_style(border_style);

    let list_items: Vec<ListItem> = items
        .iter()
        .map(|item| match item {
            SidebarItem::SourceHeader(name) => {
                let marker = if name == "local" { "●" } else { "◆" };
                ListItem::new(Line::from(Span::styled(
                    format!(" {} {} ", marker, name),
                    Theme::source_header(),
                )))
            }
            SidebarItem::GroupHeader(name) => {
                let display = shorten_path(name);
                ListItem::new(Line::from(Span::styled(
                    format!("  {} ", display),
                    Theme::project_header(),
                )))
            }
            SidebarItem::Session(idx) => {
                if let Some(session) = sessions.get(*idx) {
                    render_session_item(session, area.width)
                } else {
                    ListItem::new(Line::from(""))
                }
            }
        })
        .collect();

    let mut state = ListState::default();
    state.select(Some(selected));

    let list = List::new(list_items)
        .block(block)
        .highlight_style(Theme::selected());

    frame.render_stateful_widget(list, area, &mut state);
}

fn render_session_item(session: &AgentSession, width: u16) -> ListItem<'static> {
    let (icon, icon_style) = status_icon(&session.status);

    let time_str = session
        .started_at
        .map(format_relative_time)
        .unwrap_or_default();

    let source_marker = match &session.source {
        SessionSource::Remote { .. } => " [R]",
        SessionSource::Local => "",
    };

    let cwd_short = shorten_path(&session.cwd.to_string_lossy());

    // Calculate available width for cwd
    // Format: "  icon provider  cwd  time [R]"
    let fixed_len = 5 + session.provider.len() + 2 + time_str.len() + source_marker.len() + 2;
    let cwd_max = (width as usize).saturating_sub(fixed_len);
    let cwd_display = truncate_str(&cwd_short, cwd_max);

    ListItem::new(Line::from(vec![
        Span::styled("   ", Theme::normal()),
        Span::styled(icon.to_string(), icon_style),
        Span::styled(format!(" {} ", session.provider), Theme::normal()),
        Span::styled(cwd_display, Theme::label()),
        Span::styled(format!("  {}", time_str), Theme::label()),
        Span::styled(source_marker.to_string(), Theme::label()),
    ]))
}

fn status_icon(status: &AgentStatus) -> (&'static str, ratatui::style::Style) {
    match status {
        AgentStatus::Waiting | AgentStatus::Idle => ("●", Theme::status_active()),
        AgentStatus::Thinking => ("◐", Theme::status_thinking()),
        AgentStatus::Error => ("✖", Theme::status_error()),
        AgentStatus::Unknown => ("?", Theme::status_unknown()),
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
        assert_eq!(shorten_path("/Users/didi/Code/app"), "~/Code/app");
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
