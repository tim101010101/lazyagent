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
) {
    let hints = if confirm_mode {
        vec![("y", "confirm"), ("n", "cancel")]
    } else if search_mode {
        vec![("Enter", "apply"), ("Esc", "cancel")]
    } else {
        vec![
            ("j/k", "nav"),
            ("Enter", "attach"),
            ("n", "new"),
            ("d", "kill"),
            ("/", "search"),
            ("Tab", "group"),
            ("l/h", "detail"),
            ("q", "quit"),
        ]
    };

    let mut spans = Vec::new();

    if search_mode {
        spans.push(Span::styled(" /", Theme::title()));
        spans.push(Span::styled(search_query, Theme::value()));
        spans.push(Span::styled("  ", Theme::key_hint()));
    }

    if confirm_mode {
        spans.push(Span::styled(" Kill session? ", Theme::error()));
    }

    for (i, (key, action)) in hints.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("  ", Theme::key_hint()));
        }
        spans.push(Span::styled(*key, Theme::value()));
        spans.push(Span::styled(format!(":{}", action), Theme::key_action()));
    }

    let line = Line::from(spans);
    let help = Paragraph::new(line);
    frame.render_widget(help, area);
}
