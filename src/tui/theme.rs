use ratatui::style::{Color, Modifier, Style};

pub struct Theme;

impl Theme {
    pub fn title() -> Style {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    }

    pub fn selected() -> Style {
        Style::default()
            .bg(Color::DarkGray)
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    }

    pub fn normal() -> Style {
        Style::default().fg(Color::Gray)
    }

    pub fn source_header() -> Style {
        Style::default()
            .fg(Color::Blue)
            .add_modifier(Modifier::BOLD)
    }

    pub fn project_header() -> Style {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    }

    pub fn key_hint() -> Style {
        Style::default().fg(Color::DarkGray)
    }

    pub fn key_action() -> Style {
        Style::default().fg(Color::Gray)
    }

    pub fn label() -> Style {
        Style::default().fg(Color::DarkGray)
    }

    pub fn value() -> Style {
        Style::default().fg(Color::White)
    }

    pub fn border_focused() -> Style {
        Style::default().fg(Color::Cyan)
    }

    pub fn border_unfocused() -> Style {
        Style::default().fg(Color::DarkGray)
    }

    pub fn error() -> Style {
        Style::default().fg(Color::Red)
    }

    pub fn status_thinking() -> Style {
        Style::default().fg(Color::Yellow)
    }

    pub fn status_active() -> Style {
        Style::default().fg(Color::Green)
    }

    pub fn status_error() -> Style {
        Style::default().fg(Color::Red)
    }

    pub fn status_unknown() -> Style {
        Style::default().fg(Color::DarkGray)
    }

    pub fn passthrough_border() -> Style {
        Style::default().fg(Color::Magenta)
    }

    pub fn passthrough_indicator() -> Style {
        Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD)
    }
}
