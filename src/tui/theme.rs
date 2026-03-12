use ratatui::style::{Color, Modifier, Style};

use crate::config::ThemeConfig;

/// Theme with 19 style slots, constructed from config or defaults.
pub struct Theme {
    pub title: Style,
    pub selected: Style,
    pub selected_bar: Style,
    pub normal: Style,
    pub source_header: Style,
    pub project_header: Style,
    pub key_hint: Style,
    pub key_action: Style,
    pub label: Style,
    pub value: Style,
    pub border_focused: Style,
    pub border_unfocused: Style,
    pub error: Style,
    pub status_thinking: Style,
    pub status_active: Style,
    pub status_needs_input: Style,
    pub status_error: Style,
    pub status_unknown: Style,
    pub passthrough_border: Style,
    pub passthrough_indicator: Style,
}

impl Theme {
    pub fn from_config(cfg: &ThemeConfig) -> Self {
        Self {
            title: cfg.get_style(
                "title",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            selected: cfg.get_style(
                "selected",
                Style::default()
                    .bg(Color::DarkGray)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            selected_bar: cfg.get_style("selected_bar", Style::default().fg(Color::Cyan)),
            normal: cfg.get_style("normal", Style::default().fg(Color::Gray)),
            source_header: cfg.get_style(
                "source_header",
                Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::BOLD),
            ),
            project_header: cfg.get_style(
                "project_header",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            key_hint: cfg.get_style("key_hint", Style::default().fg(Color::DarkGray)),
            key_action: cfg.get_style("key_action", Style::default().fg(Color::Gray)),
            label: cfg.get_style("label", Style::default().fg(Color::DarkGray)),
            value: cfg.get_style("value", Style::default().fg(Color::White)),
            border_focused: cfg.get_style("border_focused", Style::default().fg(Color::Cyan)),
            border_unfocused: cfg.get_style(
                "border_unfocused",
                Style::default().fg(Color::DarkGray),
            ),
            error: cfg.get_style("error", Style::default().fg(Color::Red)),
            status_thinking: cfg.get_style("status_thinking", Style::default().fg(Color::Yellow)),
            status_active: cfg.get_style("status_active", Style::default().fg(Color::Green)),
            status_needs_input: cfg.get_style(
                "status_needs_input",
                Style::default().fg(Color::LightMagenta),
            ),
            status_error: cfg.get_style("status_error", Style::default().fg(Color::Red)),
            status_unknown: cfg.get_style("status_unknown", Style::default().fg(Color::DarkGray)),
            passthrough_border: cfg.get_style(
                "passthrough_border",
                Style::default().fg(Color::Magenta),
            ),
            passthrough_indicator: cfg.get_style(
                "passthrough_indicator",
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            ),
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::from_config(&ThemeConfig::default())
    }
}
