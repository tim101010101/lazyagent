use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::tui::theme::Theme;

const BANNER: &str = include_str!("banner.txt");

pub fn render(frame: &mut Frame, theme: &Theme) {
    let area = centered_rect(80, 80, frame.area());

    // Clear background
    frame.render_widget(Clear, area);

    // Main block
    let block = Block::default()
        .title(" Help ")
        .title_style(theme.title)
        .borders(Borders::ALL)
        .border_style(theme.border_focused);

    frame.render_widget(block, area);

    // Inner area
    let inner = area.inner(ratatui::layout::Margin {
        horizontal: 2,
        vertical: 1,
    });

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6), // Banner (6 lines)
            Constraint::Length(1), // Spacer
            Constraint::Min(0),    // Keybindings
        ])
        .split(inner);

    // Render banner (lines pre-padded to equal width in banner.txt)
    let banner_lines: Vec<Line> = BANNER
        .lines()
        .filter(|l| !l.is_empty())
        .map(|line| Line::from(Span::styled(line, theme.title)))
        .collect();
    let banner = Paragraph::new(banner_lines).alignment(Alignment::Center);
    frame.render_widget(banner, chunks[0]);

    // Render keybindings
    let keybindings = vec![
        ("Navigation", vec![
            ("j / ↓", "Move down"),
            ("k / ↑", "Move up"),
            ("g", "Jump to top"),
            ("G", "Jump to bottom"),
        ]),
        ("Session Management", vec![
            ("Enter", "Attach to session"),
            ("n", "New session"),
            ("d", "Kill session (confirm with y)"),
            ("i", "Passthrough mode (Esc+Esc to exit)"),
        ]),
        ("View", vec![
            ("l", "Show detail panel"),
            ("h", "Hide detail panel"),
            ("Tab", "Cycle grouping mode (flat/git/custom)"),
            ("/", "Search sessions"),
        ]),
        ("Other", vec![
            ("r", "Refresh sessions"),
            ("?", "Toggle this help"),
            ("q / Esc", "Quit"),
            ("Ctrl+C", "Force quit"),
        ]),
    ];

    let mut lines = Vec::new();
    for (section, keys) in keybindings {
        lines.push(Line::from(Span::styled(section, theme.source_header)));
        lines.push(Line::from(""));
        for (key, desc) in keys {
            let mut spans = Vec::new();
            spans.push(Span::styled("  ", theme.normal));
            spans.push(Span::styled(format!("{:12}", key), theme.value));
            spans.push(Span::styled(desc, theme.key_action));
            lines.push(Line::from(spans));
        }
        lines.push(Line::from(""));
    }

    let help_text = Paragraph::new(lines);
    frame.render_widget(help_text, chunks[2]);
}

/// Create a centered rect using up to certain percentage of the available rect.
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_banner_lines_equal_display_width() {
        // Test the exact same processing as render()
        let lines: Vec<&str> = BANNER.lines().filter(|l| !l.is_empty()).collect();

        assert_eq!(lines.len(), 6, "Banner should have 6 lines");

        let widths: Vec<usize> = lines.iter().map(|l| l.chars().count()).collect();
        println!("\n=== BANNER WIDTH DEBUG ===");
        for (i, w) in widths.iter().enumerate() {
            println!("Line {}: char_count={}", i, w);
        }
        println!("==========================\n");

        let first = widths[0];
        for (i, w) in widths.iter().enumerate() {
            assert_eq!(*w, first, "Line {} char count {} != line 0 count {}", i, w, first);
        }
    }
}
