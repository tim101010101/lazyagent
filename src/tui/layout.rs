use ratatui::layout::{Constraint, Direction, Layout, Rect};

pub struct AppLayout {
    pub sidebar: Rect,
    pub main: Rect,
    pub detail: Option<Rect>,
    pub help_bar: Rect,
}

impl AppLayout {
    pub fn new(area: Rect, show_detail: bool) -> Self {
        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(1)])
            .split(area);

        let main_area = vertical[0];
        let help_bar = vertical[1];

        if show_detail {
            let horizontal = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(25),
                    Constraint::Percentage(50),
                    Constraint::Percentage(25),
                ])
                .split(main_area);

            AppLayout {
                sidebar: horizontal[0],
                main: horizontal[1],
                detail: Some(horizontal[2]),
                help_bar,
            }
        } else {
            let horizontal = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
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
