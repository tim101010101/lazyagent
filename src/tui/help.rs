use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::tui::theme::Theme;

pub fn render_with_mode(
    frame: &mut Frame,
    area: Rect,
    search_mode: bool,
    search_query: &str,
    tmux_mode: bool,
    agent_running: bool,
) {
    let hints = if search_mode {
        vec![
            ("Enter", "apply"),
            ("Esc", "cancel"),
        ]
    } else if tmux_mode {
        let mut h = vec![
            ("j/k", "nav"),
            ("Enter", "resume"),
            ("l/h", "detail"),
            ("/", "search"),
        ];
        if agent_running {
            h.push(("prefix+\u{2190}", "agent"));
        }
        h.push(("q", "quit"));
        h
    } else {
        vec![
            ("j/k", "nav"),
            ("Enter", "resume"),
            ("l", "detail"),
            ("h", "hide"),
            ("/", "search"),
            ("q", "quit"),
        ]
    };

    let mut spans = Vec::new();

    if search_mode {
        spans.push(Span::styled(" /", Theme::title()));
        spans.push(Span::styled(search_query, Theme::value()));
        spans.push(Span::styled("  ", Theme::key_hint()));
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
