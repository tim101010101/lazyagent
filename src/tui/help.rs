use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::tui::theme::Theme;

pub fn render(
    frame: &mut Frame,
    area: Rect,
    search_mode: bool,
    search_query: &str,
    confirm_mode: bool,
    passthrough_mode: bool,
    theme: &Theme,
) {
    let hints = if passthrough_mode {
        vec![("Esc+Esc", "exit passthrough")]
    } else if confirm_mode {
        vec![("y", "confirm"), ("n", "cancel")]
    } else if search_mode {
        vec![("Enter", "apply"), ("Esc", "cancel")]
    } else {
        vec![("?", "help"), ("q", "quit")]
    };

    let mut spans = Vec::new();

    // Mode badge
    if passthrough_mode {
        spans.push(Span::styled("▐ PASS ▌ ", theme.passthrough_indicator));
    } else if confirm_mode {
        spans.push(Span::styled("▐ CONFIRM ▌ ", theme.error));
    } else if search_mode {
        spans.push(Span::styled("▐ SEARCH ▌ ", theme.status_thinking));
    } else {
        spans.push(Span::styled("▐ NORMAL ▌ ", theme.label));
    }

    if search_mode {
        spans.push(Span::styled("/", theme.title));
        spans.push(Span::styled(search_query, theme.value));
        spans.push(Span::styled("  ", theme.key_hint));
    }

    for (i, (key, action)) in hints.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("  ", theme.key_hint));
        }
        spans.push(Span::styled(*key, theme.value));
        spans.push(Span::styled(format!(":{}", action), theme.key_action));
    }

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(20)])
        .split(area);

    frame.render_widget(Paragraph::new(Line::from(spans)), cols[0]);

    let version = env!("CARGO_PKG_VERSION");
    let ver_text = format!("v{} ", version);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(ver_text, theme.label)))
            .alignment(ratatui::layout::Alignment::Right),
        cols[1],
    );
}
