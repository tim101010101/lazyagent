use ratatui::{
    layout::Rect,
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

    if search_mode {
        spans.push(Span::styled(" /", theme.title));
        spans.push(Span::styled(search_query, theme.value));
        spans.push(Span::styled("  ", theme.key_hint));
    }

    if passthrough_mode {
        spans.push(Span::styled(" PASSTHROUGH ", theme.passthrough_indicator));
    }

    if confirm_mode {
        spans.push(Span::styled(" Kill session? ", theme.error));
    }

    for (i, (key, action)) in hints.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("  ", theme.key_hint));
        }
        spans.push(Span::styled(*key, theme.value));
        spans.push(Span::styled(format!(":{}", action), theme.key_action));
    }

    let line = Line::from(spans);
    let help = Paragraph::new(line);
    frame.render_widget(help, area);
}
