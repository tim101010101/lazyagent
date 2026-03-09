use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::config::LayoutConfig;

pub struct AppLayout {
    pub sidebar: Rect,
    pub main: Rect,
    pub detail: Option<Rect>,
    pub help_bar: Rect,
}

impl AppLayout {
    pub fn new(area: Rect, show_detail: bool, layout_config: &LayoutConfig) -> Self {
        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(1)])
            .split(area);

        let main_area = vertical[0];
        let help_bar = vertical[1];

        if show_detail {
            let detail_percent =
                100u16.saturating_sub(layout_config.sidebar_percent + layout_config.main_percent);
            let horizontal = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(layout_config.sidebar_percent),
                    Constraint::Percentage(layout_config.main_percent),
                    Constraint::Percentage(detail_percent),
                ])
                .split(main_area);

            AppLayout {
                sidebar: horizontal[0],
                main: horizontal[1],
                detail: Some(horizontal[2]),
                help_bar,
            }
        } else {
            let main_2col = 100u16.saturating_sub(layout_config.sidebar_2col_percent);
            let horizontal = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(layout_config.sidebar_2col_percent),
                    Constraint::Percentage(main_2col),
                ])
                .split(main_area);

            AppLayout {
                sidebar: horizontal[0],
                main: horizontal[1],
                detail: None,
                help_bar,
            }
        }
    }
}
