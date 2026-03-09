use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};

use crate::app::SidebarItem;
use crate::tui::theme::Theme;

pub fn render(
    frame: &mut Frame,
    area: Rect,
    items: &[SidebarItem],
    selected: usize,
    focused: bool,
) {
    let border_style = if focused {
        Theme::border_focused()
    } else {
        Theme::border_unfocused()
    };

    let block = Block::default()
        .title(" Sessions ")
        .title_style(Theme::title())
        .borders(Borders::ALL)
        .border_style(border_style);

    let list_items: Vec<ListItem> = items
        .iter()
        .map(|item| match item {
            SidebarItem::ProjectHeader(name) => {
                let display = shorten_path(name);
                ListItem::new(Line::from(Span::styled(
                    format!(" {} ", display),
                    Theme::project_header(),
                )))
            }
            SidebarItem::Session(summary) => {
                let time_str = summary
                    .updated_at
                    .map(|t| format_relative_time(t))
                    .unwrap_or_default();

                let title = truncate_str(&summary.title, area.width.saturating_sub(time_str.len() as u16 + 6) as usize);

                ListItem::new(Line::from(vec![
                    Span::styled("  ", Theme::normal()),
                    Span::styled(title, Theme::normal()),
                    Span::styled(
                        format!(" {}", time_str),
                        Theme::label(),
                    ),
                ]))
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

fn shorten_path(path: &str) -> String {
    // Show last two components: ~/Code/app → Code/app
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if parts.len() <= 2 {
        return path.to_string();
    }
    format!("~/{}", parts[parts.len() - 2..].join("/"))
}

fn truncate_str(s: &str, max_len: usize) -> String {
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

pub fn format_relative_time(millis: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    let diff_secs = (now - millis) / 1000;

    if diff_secs < 0 {
        return "just now".into();
    }

    let diff_mins = diff_secs / 60;
    let diff_hours = diff_mins / 60;
    let diff_days = diff_hours / 24;

    if diff_secs < 60 {
        "just now".into()
    } else if diff_mins < 60 {
        format!("{}m ago", diff_mins)
    } else if diff_hours < 24 {
        format!("{}h ago", diff_hours)
    } else if diff_days < 30 {
        format!("{}d ago", diff_days)
    } else {
        format!("{}mo ago", diff_days / 30)
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
        // Must not panic on CJK characters
        assert_eq!(truncate_str("修复图片加载失败显示问题", 6), "修复图...");
    }

    #[test]
    fn test_format_relative_time() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        assert_eq!(format_relative_time(now), "just now");
        assert_eq!(format_relative_time(now - 120_000), "2m ago");
        assert_eq!(format_relative_time(now - 7_200_000), "2h ago");
        assert_eq!(format_relative_time(now - 172_800_000), "2d ago");
    }
}
